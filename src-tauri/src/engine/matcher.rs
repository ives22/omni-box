use base64::Engine;
use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchableItem {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub category: String, // "plugin" | "app" | "command" | "file" | "totp" | "vault"
    pub icon: String,
    pub icon_data_url: Option<String>,
    pub keywords: Vec<String>,
    pub action_payload: String,
    #[serde(skip)]
    pub pinyin_full: String,
    #[serde(skip)]
    pub pinyin_initials: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub category: String,
    pub icon: String,
    pub icon_data_url: Option<String>,
    pub keywords: Vec<String>,
    pub action_payload: String,
    pub score: i64,
    pub match_type: String,
    pub badge: Option<String>,
}

pub fn compute_pinyin(text: &str) -> (String, String) {
    let mut full = String::new();
    let mut initials = String::new();

    for ch in text.chars() {
        if let Some(py) = ch.to_pinyin() {
            let plain = py.plain();
            full.push_str(plain);
            if let Some(first) = plain.chars().next() {
                initials.push(first);
            }
        } else if ch.is_ascii_alphanumeric() {
            let lower = ch.to_ascii_lowercase();
            full.push(lower);
            initials.push(lower);
        }
    }
    (full, initials)
}

impl SearchableItem {
    pub fn new(
        id: &str,
        title: &str,
        subtitle: &str,
        category: &str,
        icon: &str,
        icon_data_url: Option<String>,
        keywords: Vec<String>,
        action_payload: &str,
    ) -> Self {
        let mut combined = title.to_string();
        for kw in &keywords {
            combined.push(' ');
            combined.push_str(kw);
        }
        let (pinyin_full, pinyin_initials) = compute_pinyin(&combined);
        Self {
            id: id.to_string(),
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            category: category.to_string(),
            icon: icon.to_string(),
            icon_data_url,
            keywords,
            action_payload: action_payload.to_string(),
            pinyin_full,
            pinyin_initials,
        }
    }
}

/// Subsequence fuzzy scoring (Raycast / Spotlight style)
fn subsequence_score(haystack: &str, needle: &str) -> Option<i64> {
    if needle.is_empty() {
        return Some(10);
    }
    let h_chars: Vec<char> = haystack.chars().collect();
    let n_chars: Vec<char> = needle.chars().collect();
    if n_chars.len() > h_chars.len() {
        return None;
    }

    let mut score: i64 = 0;
    let mut h_idx = 0;
    let mut consecutive = 0;

    for (n_i, &nc) in n_chars.iter().enumerate() {
        let mut found = false;
        while h_idx < h_chars.len() {
            if h_chars[h_idx] == nc {
                found = true;
                if n_i == 0 && h_idx == 0 {
                    score += 80;
                }
                if consecutive > 0 {
                    score += 35 * consecutive;
                } else {
                    score += 15;
                }
                consecutive += 1;
                h_idx += 1;
                break;
            } else {
                consecutive = 0;
                h_idx += 1;
            }
        }
        if !found {
            return None;
        }
    }

    score += (100_i64).saturating_sub(h_chars.len() as i64);
    Some(score)
}

pub fn search_items(
    items: &[SearchableItem],
    query: &str,
    frecency_map: &HashMap<String, i64>,
) -> Vec<SearchResultItem> {
    let q = query.trim().to_lowercase();
    let mut results = Vec::new();

    for item in items {
        let frecency_bonus = *frecency_map.get(&item.id).unwrap_or(&0) * 25;
        let badge = match item.category.as_str() {
            "plugin" => Some("核心插件".to_string()),
            "app" => Some("本机应用".to_string()),
            "command" => Some("系统指令".to_string()),
            "file" => Some("本地文件".to_string()),
            _ => None,
        };

        if q.is_empty() {
            let base = match item.category.as_str() {
                "plugin" => 500,
                "app" => 220,
                "command" => 150,
                _ => 100,
            };
            results.push(SearchResultItem {
                id: item.id.clone(),
                title: item.title.clone(),
                subtitle: item.subtitle.clone(),
                category: item.category.clone(),
                icon: item.icon.clone(),
                icon_data_url: item.icon_data_url.clone(),
                keywords: item.keywords.clone(),
                action_payload: item.action_payload.clone(),
                score: base + frecency_bonus,
                match_type: "default".to_string(),
                badge,
            });
            continue;
        }

        let title_lower = item.title.to_lowercase();
        let mut best_score: Option<(i64, &str)> = None;

        if title_lower == q {
            best_score = Some((1200, "精准匹配"));
        } else if title_lower.starts_with(&q) {
            best_score = Some((960, "前缀匹配"));
        } else if title_lower.contains(&q) {
            best_score = Some((780, "名称匹配"));
        }

        if best_score.is_none() {
            for kw in &item.keywords {
                let kw_l = kw.to_lowercase();
                if kw_l == q {
                    best_score = Some((1120, "指令直达"));
                    break;
                } else if kw_l.starts_with(&q) {
                    best_score = Some((890, "关键词前缀"));
                    break;
                } else if kw_l.contains(&q) {
                    best_score = Some((700, "关键词"));
                }
            }
        }

        if best_score.is_none() {
            if let Some(s) = subsequence_score(&item.pinyin_initials, &q) {
                best_score = Some((620 + s, "拼音首字母"));
            }
        }

        if best_score.is_none() {
            if let Some(s) = subsequence_score(&item.pinyin_full, &q) {
                best_score = Some((490 + s, "全拼匹配"));
            }
        }

        if let Some((score, m_type)) = best_score {
            let category_bonus = if item.category == "plugin" { 50 } else { 0 };
            results.push(SearchResultItem {
                id: item.id.clone(),
                title: item.title.clone(),
                subtitle: item.subtitle.clone(),
                category: item.category.clone(),
                icon: item.icon.clone(),
                icon_data_url: item.icon_data_url.clone(),
                keywords: item.keywords.clone(),
                action_payload: item.action_payload.clone(),
                score: score + category_bonus + frecency_bonus,
                match_type: m_type.to_string(),
                badge,
            });
        }
    }

    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.truncate(18);
    results
}

/// Official clean Chinese display names ONLY for macOS built-in apps & native Chinese apps.
/// English-named apps (PyCharm, GoLand, VS Code, Cursor, Docker, etc.) keep their exact original name!
fn lookup_official_zh_display_name(en_stem: &str) -> Option<&'static str> {
    let lower = en_stem.trim().to_lowercase();
    match lower.as_str() {
        // Apple macOS Built-in Apps
        "system settings" | "system preferences" => Some("系统设置"),
        "activity monitor" => Some("活动监视器"),
        "terminal" => Some("终端"),
        "finder" => Some("访达"),
        "automator" => Some("自动操作"),
        "calculator" => Some("计算器"),
        "calendar" => Some("日历"),
        "notes" => Some("备忘录"),
        "reminders" => Some("提醒事项"),
        "mail" => Some("邮件"),
        "messages" => Some("信息"),
        "maps" => Some("地图"),
        "photos" => Some("照片"),
        "preview" => Some("预览"),
        "music" => Some("音乐"),
        "podcasts" => Some("播客"),
        "tv" => Some("视频"),
        "books" => Some("图书"),
        "dictionary" => Some("词典"),
        "font book" => Some("字体册"),
        "keychain access" => Some("钥匙串访问"),
        "disk utility" => Some("磁盘工具"),
        "console" => Some("控制台"),
        "screenshot" => Some("截屏"),
        "textedit" => Some("文本编辑"),
        "time machine" => Some("时间机器"),
        "weather" => Some("天气"),
        "stocks" => Some("股市"),
        "clock" => Some("时钟"),
        "voice memos" => Some("语音备忘录"),
        "passwords" => Some("密码"),
        "freeform" => Some("无边记"),
        "shortcuts" => Some("快捷指令"),
        "migration assistant" => Some("迁移助理"),
        "system information" => Some("系统信息"),
        "digital color meter" => Some("数码测色计"),
        "audio midi setup" => Some("音频 MIDI 设置"),
        "bluetooth file exchange" => Some("蓝牙文件交换"),
        "screen sharing" => Some("屏幕共享"),

        // Native Chinese Apps (only used if mdls didn't already provide the localized name)
        "wechat" => Some("微信"),
        "wechatwebdevtools" | "wechatdevtools" => Some("微信开发者工具"),
        "wecom" | "wxwork" => Some("企业微信"),
        "dingtalk" => Some("钉钉"),
        "feishu" | "lark" => Some("飞书"),
        "tencentmeeting" => Some("腾讯会议"),
        "neteasemusic" => Some("网易云音乐"),
        "qqmusic" => Some("QQ音乐"),
        "yuque" => Some("语雀"),
        "baidunetdisk" | "baidunetdisk_mac" => Some("百度网盘"),
        "adrive" | "aliyundrive" => Some("阿里云盘"),
        "thunder" => Some("迅雷"),
        "bilibili" => Some("哔哩哔哩"),
        "jianyingpro" => Some("剪映专业版"),
        "uuremote" => Some("UU远程"),
        "mouseboost pro" | "mouseboost" => Some("右键助手专业版"),
        _ => None,
    }
}

/// Hidden search keywords so users can search apps by Chinese/English category terms
/// without polluting the clean application display title!
fn lookup_extra_search_keywords(en_stem: &str) -> &'static [&'static str] {
    let lower = en_stem.trim().to_lowercase();
    match lower.as_str() {
        "wechatwebdevtools" | "wechatdevtools" => &["微信开发者工具", "微信", "小程序", "wx"],
        "wechat" => &["微信", "wx", "weixin"],
        "dingtalk" => &["钉钉", "dd"],
        "feishu" | "lark" | "larksuite" => &["飞书", "lark", "fs"],
        "pycharm" | "pycharm ce" => &["python", "ide", "py"],
        "goland" => &["go", "golang", "ide"],
        "webstorm" => &["web", "js", "ts", "前端", "ide"],
        "intellij idea" | "intellij idea ce" => &["java", "idea", "ide"],
        "visual studio code" | "code" => &["vscode", "code", "编辑器"],
        "cursor" => &["ai", "编辑器", "code"],
        "sublime text" => &["文本编辑器", "编辑器"],
        "typora" => &["markdown", "md", "笔记"],
        "obsidian" => &["笔记", "知识库", "markdown"],
        "bob" => &["翻译", "ocr", "词典"],
        "ishot pro" | "ishot" => &["截图", "录屏", "贴图"],
        "fastzip" => &["解压", "压缩", "zip", "rar"],
        "cleanmymac_5" | "cleanmymac" => &["清理", "卸载", "cleanmymac"],
        "uuremote" => &["uu远程", "远程桌面", "远控"],
        "navicat premium" | "navicat" => &["数据库", "mysql", "postgres", "sql"],
        "another redis desktop manager" => &["redis", "数据库"],
        "prettyzoo" => &["zookeeper", "zk"],
        "apifox" | "postman" => &["接口", "api", "调试"],
        "wireshark" | "charles" => &["抓包", "网络分析"],
        "securecrt" | "iterm" | "iterm2" | "warp" => &["终端", "ssh", "命令行"],
        "docker" => &["容器", "镜像"],
        "clash verge" | "clashx" | "astrill" | "openvpn connect" => &["代理", "vpn", "网络"],
        "google chrome" => &["chrome", "谷歌浏览器", "浏览器"],
        "microsoft edge" => &["edge", "浏览器"],
        "safari" => &["safari", "浏览器"],
        "telegram" => &["电报", "tg"],
        _ => &[],
    }
}

/// Batch query macOS Spotlight (`mdls`) for localized `kMDItemDisplayName` of all .app bundles in one shot
fn batch_query_mdls_display_names(
    paths: &[PathBuf],
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if paths.is_empty() {
        return map;
    }

    let mut cmd = Command::new("mdls");
    cmd.arg("-name")
        .arg("kMDItemFSName")
        .arg("-name")
        .arg("kMDItemDisplayName");
    for p in paths {
        cmd.arg(p);
    }

    if let Ok(output) = cmd.output() {
        let text = String::from_utf8_lossy(&output.stdout);
        let mut current_display: Option<String> = None;
        let mut current_fs: Option<String> = None;

        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("kMDItemDisplayName") {
                if let Some(eq_idx) = rest.find('=') {
                    let val = rest[eq_idx + 1..].trim().trim_matches('"').trim();
                    if !val.is_empty() && val != "(null)" {
                        current_display = Some(val.trim_end_matches(".app").to_string());
                    } else {
                        current_display = None;
                    }
                }
            } else if let Some(rest) = trimmed.strip_prefix("kMDItemFSName") {
                if let Some(eq_idx) = rest.find('=') {
                    let val = rest[eq_idx + 1..].trim().trim_matches('"').trim();
                    if !val.is_empty() && val != "(null)" {
                        current_fs = Some(val.to_string());
                    } else {
                        current_fs = None;
                    }
                }
            }

            if let (Some(fs_name), Some(disp_name)) = (&current_fs, &current_display) {
                map.insert(fs_name.clone(), disp_name.clone());
                current_fs = None;
                current_display = None;
            }
        }
    }
    map
}

/// Extract or read cached 64x64 PNG icon for a macOS .app bundle
pub fn extract_app_icon_base64(app_path: &Path, cache_dir: &Path) -> Option<String> {
    let stem = app_path.file_stem()?.to_str()?;
    let safe_name: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let cached_png = cache_dir.join(format!("{}.png", safe_name));

    if !cached_png.exists() {
        let resources_dir = app_path.join("Contents").join("Resources");
        if !resources_dir.exists() {
            return None;
        }
        // Find the primary .icns file inside Contents/Resources
        let mut best_icns: Option<PathBuf> = None;
        if let Ok(entries) = fs::read_dir(&resources_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("icns") {
                    let fname = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                    if fname.contains("appicon") || fname.contains("app") || best_icns.is_none() {
                        best_icns = Some(p.clone());
                        if fname.contains("appicon") {
                            break;
                        }
                    }
                }
            }
        }

        if let Some(icns_path) = best_icns {
            let _ = fs::create_dir_all(cache_dir);
            let _ = Command::new("sips")
                .args([
                    "-s",
                    "format",
                    "png",
                    "-z",
                    "64",
                    "64",
                    &icns_path.to_string_lossy(),
                    "--out",
                    &cached_png.to_string_lossy(),
                ])
                .output();
        }
    }

    if cached_png.exists() {
        if let Ok(bytes) = fs::read(&cached_png) {
            if !bytes.is_empty() {
                let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                return Some(format!("data:image/png;base64,{}", b64));
            }
        }
    }
    None
}

/// Scan macOS application directories and build bilingual searchable items
pub fn scan_system_applications(cache_dir: &Path) -> Vec<SearchableItem> {
    let mut apps = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();
    let mut candidate_paths = Vec::new();

    let mut app_dirs = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from("/System/Library/CoreServices/Applications"),
    ];

    if let Some(home) = dirs::home_dir() {
        app_dirs.push(home.join("Applications"));
        app_dirs.push(home.join("Applications/Chrome Apps.localized"));
    }

    for dir in app_dirs {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("app") {
                    candidate_paths.push(p);
                } else if p.is_dir() {
                    // Also check 1-level subdirectory inside /Applications (e.g. Utilities, iNodeClient, OpenVPN Connect)
                    if let Ok(sub_entries) = fs::read_dir(&p) {
                        for sub in sub_entries.flatten() {
                            let sp = sub.path();
                            if sp.extension().and_then(|e| e.to_str()) == Some("app") {
                                candidate_paths.push(sp);
                            }
                        }
                    }
                }
            }
        }
    }

    // Batch-query macOS Spotlight for localized kMDItemDisplayName (e.g. wechatwebdevtools.app -> "微信开发者工具")
    let mdls_display_map = batch_query_mdls_display_names(&candidate_paths);

    for p in candidate_paths {
        if let Some(item) = parse_app_bundle(&p, cache_dir, &mut seen_paths, &mdls_display_map) {
            apps.push(item);
        }
    }
    apps
}

fn parse_app_bundle(
    p: &Path,
    cache_dir: &Path,
    seen: &mut std::collections::HashSet<String>,
    mdls_display_map: &std::collections::HashMap<String, String>,
) -> Option<SearchableItem> {
    let stem = p.file_stem()?.to_str()?;
    let file_name = p.file_name()?.to_str()?;
    let full_path = p.to_string_lossy().to_string();
    if seen.contains(&full_path) {
        return None;
    }
    seen.insert(full_path.clone());

    let id = format!("app:{}", stem.to_lowercase().replace(' ', "-"));
    let dict_zh = lookup_official_zh_display_name(stem);
    let mdls_name = mdls_display_map
        .get(file_name)
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty());

    // Keep clean, exact application name (e.g. "PyCharm", "GoLand", "微信开发者工具", "飞书")
    // without appending parenthesized stem or descriptive suffixes!
    let display_title = match mdls_name {
        Some(name) if !name.eq_ignore_ascii_case(stem) => name.to_string(),
        _ => dict_zh.unwrap_or(stem).to_string(),
    };

    let mut keywords = vec![stem.to_string(), "app".to_string(), "应用".to_string()];
    if let Some(name) = mdls_name {
        if !keywords.iter().any(|k| k.eq_ignore_ascii_case(name)) {
            keywords.push(name.to_string());
        }
    }
    if let Some(zh) = dict_zh {
        if !keywords.iter().any(|k| k == zh) {
            keywords.push(zh.to_string());
        }
    }
    for extra_kw in lookup_extra_search_keywords(stem) {
        if !keywords.iter().any(|k| k.eq_ignore_ascii_case(extra_kw)) {
            keywords.push((*extra_kw).to_string());
        }
    }

    let icon_data_url = extract_app_icon_base64(p, cache_dir);

    Some(SearchableItem::new(
        &id,
        &display_title,
        &full_path,
        "app",
        "AppWindow",
        icon_data_url,
        keywords,
        &full_path,
    ))
}

/// Spotlight local file search via macOS `mdfind` with streaming early-termination & scoped folders
pub fn search_spotlight_files(query: &str) -> Vec<SearchResultItem> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::Instant;

    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/Users"));

    // Scope mdfind to user-meaningful directories so it never crawls ~/Library or ~/.cargo
    let candidate_dirs = [
        home.join("Desktop"),
        home.join("Documents"),
        home.join("Downloads"),
        home.join("Developer"),
        home.join("Projects"),
        home.join("Work"),
        home.join(".gemini/antigravity/scratch"),
    ];

    let mut cmd = Command::new("mdfind");
    let mut added_dir = false;
    for d in &candidate_dirs {
        if d.exists() {
            cmd.arg("-onlyin").arg(d);
            added_dir = true;
        }
    }
    if !added_dir {
        cmd.arg("-onlyin").arg(&home);
    }

    cmd.arg("-name").arg(q);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut items = Vec::new();
    if let Ok(mut child) = cmd.spawn() {
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let start_time = Instant::now();

            for (raw_idx, line_res) in reader.lines().enumerate() {
                // Hard time budget (200ms max) & line budget (250 lines max) to guarantee zero lag
                if items.len() >= 18 || raw_idx > 250 || start_time.elapsed().as_millis() > 200 {
                    break;
                }
                if let Ok(line) = line_res {
                    if line.contains("/Library/")
                        || line.contains("/node_modules/")
                        || line.contains("/.git/")
                        || line.contains("/target/debug/")
                        || line.contains("/dist/")
                    {
                        continue;
                    }
                    let path = Path::new(&line);
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&line)
                        .to_string();
                    let is_dir = path.is_dir();
                    let icon = if is_dir { "FolderGit2" } else { "FileCode2" };
                    let badge = if is_dir { "文件夹" } else { "本地文件" };
                    let idx = items.len();

                    items.push(SearchResultItem {
                        id: format!("file:{}:{}", idx, file_name),
                        title: file_name.clone(),
                        subtitle: line.clone(),
                        category: "file".to_string(),
                        icon: icon.to_string(),
                        icon_data_url: None,
                        keywords: vec![file_name],
                        action_payload: line,
                        score: 900 - (idx as i64 * 10),
                        match_type: "Spotlight 文件".to_string(),
                        badge: Some(badge.to_string()),
                    });
                }
            }
        }
        // Immediately terminate mdfind subprocess so it never hangs in the background
        let _ = child.kill();
        let _ = child.wait();
    }
    items
}
