// src-tauri/src/lib.rs
use std::process::Command;
use std::sync::mpsc;
use std::thread;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use file_icon_provider::get_file_icon;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri::Manager;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, POINT, TRUE};
use windows::core::Interface;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile, STGM_READ};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    mouse_event, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetClassNameW, GetCursorPos, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetCursorPos, SetForegroundWindow,
    SetWindowPos, ShowWindow, SPI_GETDESKWALLPAPER, SW_HIDE, SW_RESTORE, SW_SHOW,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SystemParametersInfoW,
};

// ============================================================
// 自定义命令
// ============================================================

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// 获取桌面文件列表
#[tauri::command]
fn get_desktop_items() -> Result<Vec<std::path::PathBuf>, String> {
    let desktop_path = dirs::desktop_dir().ok_or("无法找到桌面目录")?;
    let entries = std::fs::read_dir(desktop_path).map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        paths.push(path);
    }
    Ok(paths)
}

// 启动文件监听器
#[tauri::command]
fn start_watcher(app_handle: tauri::AppHandle) -> Result<(), String> {
    let desktop_path = dirs::desktop_dir().ok_or("无法找到桌面目录")?;
    thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        let mut watcher =
            RecommendedWatcher::new(tx, Config::default()).unwrap();
        watcher
            .watch(&desktop_path, RecursiveMode::NonRecursive)
            .unwrap();
        for res in rx {
            match res {
                Ok(_event) => {
                    let _ = app_handle.emit("files-changed", ());
                }
                Err(e) => eprintln!("监听错误: {:?}", e),
            }
        }
    });
    Ok(())
}

// 获取文件系统图标的 base64 数据 URI
#[tauri::command]
fn get_file_icon_base64(path: String, size: Option<u16>) -> Result<String, String> {
    let size = size.unwrap_or(64);
    let icon =
        get_file_icon(&path, size).map_err(|e| format!("获取图标失败: {:?}", e))?;

    let mut png_buf = std::io::Cursor::new(Vec::new());
    let encoder = PngEncoder::new(&mut png_buf);
    encoder
        .write_image(
            &icon.pixels,
            icon.width,
            icon.height,
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("PNG 编码失败: {}", e))?;

    let b64 = BASE64.encode(png_buf.into_inner());
    Ok(format!("data:image/png;base64,{}", b64))
}

// 获取 Windows 系统壁纸的 base64 数据 URI
#[tauri::command]
fn get_wallpaper_base64() -> Result<String, String> {
    unsafe {
        let mut buf = [0u16; 260];
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            260,
            Some(buf.as_mut_ptr() as *mut _),
            Default::default(),
        )
        .map_err(|e| format!("获取壁纸路径失败: {}", e))?;

        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        if len == 0 {
            return Err("未设置壁纸".into());
        }
        let path = String::from_utf16_lossy(&buf[..len]);

        let bytes = std::fs::read(&path).map_err(|e| format!("读取壁纸文件失败: {}", e))?;
        let b64 = BASE64.encode(&bytes);

        // 根据扩展名判断 MIME
        let ext = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg")
            .to_lowercase();
        let mime = match ext.as_str() {
            "png" => "image/png",
            "bmp" => "image/bmp",
            "gif" => "image/gif",
            _ => "image/jpeg",
        };

        Ok(format!("data:{};base64,{}", mime, b64))
    }
}

/// 获取 Windows 用户名
#[tauri::command]
fn get_username() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "User".into())
}

// ---- 自定义分类配置持久化 ----

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CategoryConfig {
    categories: HashMap<String, String>,
    file_categories: HashMap<String, String>,
}

impl Default for CategoryConfig {
    fn default() -> Self {
        Self {
            categories: HashMap::new(),
            file_categories: HashMap::new(),
        }
    }
}

fn get_config_path(app_handle: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("category_config.json"))
}

#[tauri::command]
fn load_category_config(app_handle: tauri::AppHandle) -> Result<CategoryConfig, String> {
    let path = get_config_path(&app_handle)?;
    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    } else {
        Ok(CategoryConfig::default())
    }
}

#[tauri::command]
fn save_category_config(app_handle: tauri::AppHandle, config: CategoryConfig) -> Result<(), String> {
    let path = get_config_path(&app_handle)?;
    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

// ============================================================
// 开始菜单分类系统
// ============================================================

use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::OnceLock;

/// Programs 文件夹 → 8 个分类的映射
const FOLDER_CATEGORIES: &[(&[&str], &str, &str)] = &[
    (&["Microsoft Office 工具", "WPS Office", "Accessories"], "高效工作", "💼"),
    (&["Adobe", "剪映专业版", "美图", "FlashCenter", "CC Switch"], "创造力", "🎨"),
    (&["Android Studio", "Burp Suite Professional", "Git", "Java", "Java Development Kit",
      "JetBrains", "Node.js", "PowerToys (Preview)", "Python 3.13", "Ubuntu",
      "Visual Studio 2022", "Visual Studio Code", "Windows Kits", "微信开发者工具",
      "Tesseract-OCR", "Paradox Interactive"], "开发人员工具", "⚙️"),
    (&["AMD", "AMD Bug Report Tool", "Clash Verge", "Everything", "GeoGebra", "Google Chrome",
      "IObit Uninstaller", "Internet Download Manager", "Mem Reduct", "Microsoft Edge",
      "Neat Download Manager", "Notepad++", "OneDrive", "OpenVPN", "Radmin Server 3",
      "Radmin VPN", "TAP-Windows", "VMware", "WinRAR", "WindowsCleaner", "Xshell 8",
      "小米", "希沃软件", "雷电多开器", "雷电模拟器9", "biubiu",
      "AMD Software꞉ Adrenalin Edition"], "实用程序与工具", "🔧"),
    (&["Steam", "腾讯游戏", "Roblox"], "娱乐", "🎮"),
    (&["微信", "企业微信", "腾讯会议", "KOOK", "腾讯软件", "Flash Center"], "社交", "💬"),
    (&["Accessibility"], "辅助功能", "♿"),
    (&["Administrative Tools", "Maintenance", "System Tools", "Windows PowerShell",
      "Startup", "Unknown", "Accessories (系统)"], "其他", "📁"),
];

/// 获取 Programs 文件夹名 → 分类名称的映射表
fn folder_to_category() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    for (folders, cat, _) in FOLDER_CATEGORIES {
        for folder in *folders {
            map.insert(*folder, *cat);
        }
    }
    map
}

/// 解析 .lnk 快捷方式，返回目标 exe 名称（如 "chrome.exe"）
fn get_lnk_target_exe(path: &Path) -> Option<String> {
    unsafe {
        // 初始化 COM (如果已初始化则忽略)
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let shell_link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;

        let persist: IPersistFile = shell_link.cast().ok()?;

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        persist
            .Load(windows::core::PCWSTR(wide.as_ptr()), STGM_READ)
            .ok()?;

        let mut buf = [0u16; 260];
        shell_link
            .GetPath(&mut buf, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)
            .ok()?;

        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        if len == 0 {
            return None;
        }
        let target = String::from_utf16_lossy(&buf[..len]);
        let target_path = Path::new(&target);
        target_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    }
}

/// 扫描 Programs 目录，建立 exe 名称 → 分类的索引
fn build_exe_index() -> HashMap<String, &'static str> {
    let mut index = HashMap::new();
    let folder_map = folder_to_category();

    let dirs = {
        let mut d: Vec<String> = Vec::new();
        d.push(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs".into());
        if let Ok(appdata) = std::env::var("APPDATA") {
            d.push(format!("{}\\Microsoft\\Windows\\Start Menu\\Programs", appdata));
        }
        d
    };

    for dir_path in &dirs {
        let base = match std::fs::read_dir(dir_path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for entry in base.flatten() {
            let path = entry.path();
            let is_lnk = path.extension().map_or(false, |e| {
                e.eq_ignore_ascii_case("lnk")
            });
            let is_dir = path.is_dir();

            if is_lnk {
                // 直接在 Programs 根目录的 .lnk → 归入"其他"
                if let Some(exe) = get_lnk_target_exe(&path) {
                    index.insert(exe.to_lowercase(), "其他");
                }
            } else if is_dir {
                // 子文件夹 → 每个 .lnk 归入该文件夹对应的分类
                let folder_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let category = folder_map
                    .get(folder_name.as_str())
                    .copied()
                    .unwrap_or("其他");

                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    for sub in sub_entries.flatten() {
                        let sub_path = sub.path();
                        if sub_path.extension().map_or(false, |e| {
                            e.eq_ignore_ascii_case("lnk")
                        }) {
                            if let Some(exe) = get_lnk_target_exe(&sub_path) {
                                index.insert(exe.to_lowercase(), category);
                            }
                        }
                    }
                }
            }
        }
    }

    // 手动添加一些常见 exe 的兜底映射
    let fallback_map: &[(&str, &str)] = &[
        ("chrome.exe", "实用程序与工具"),
        ("msedge.exe", "实用程序与工具"),
        ("firefox.exe", "实用程序与工具"),
        ("notepad++.exe", "实用程序与工具"),
        ("everything.exe", "实用程序与工具"),
        ("winrar.exe", "实用程序与工具"),
        ("wechat.exe", "社交"),
        ("微信.exe", "社交"),
        ("qq.exe", "社交"),
        ("tim.exe", "社交"),
        ("discord.exe", "社交"),
        ("slack.exe", "社交"),
        ("dingtalk.exe", "社交"),
        ("steam.exe", "娱乐"),
        ("spotify.exe", "娱乐"),
        ("code.exe", "开发人员工具"),
        ("clion64.exe", "开发人员工具"),
        ("idea64.exe", "开发人员工具"),
        ("webstorm64.exe", "开发人员工具"),
        ("goland64.exe", "开发人员工具"),
        ("pycharm64.exe", "开发人员工具"),
        ("git.exe", "开发人员工具"),
        ("node.exe", "开发人员工具"),
        ("python.exe", "开发人员工具"),
        ("java.exe", "开发人员工具"),
        ("javaw.exe", "开发人员工具"),
        ("winword.exe", "高效工作"),
        ("excel.exe", "高效工作"),
        ("powerpnt.exe", "高效工作"),
        ("outlook.exe", "高效工作"),
        ("wps.exe", "高效工作"),
        ("wpp.exe", "高效工作"),
        ("et.exe", "高效工作"),
        ("photoshop.exe", "创造力"),
        ("audition.exe", "创造力"),
        ("premiere.exe", "创造力"),
        ("afterfx.exe", "创造力"),
        ("illustrator.exe", "创造力"),
    ];
    for (exe, cat) in fallback_map {
        index.entry(exe.to_string()).or_insert(cat);
    }

    index
}

/// 获取缓存的分索引
fn get_exe_index() -> &'static HashMap<String, &'static str> {
    static INDEX: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let index = build_exe_index();
        eprintln!(
            "[StartMenu] 已索引 {} 个可执行程序",
            index.len()
        );
        index
    })
}

/// 判断桌面文件的分类
fn categorize_desktop_item(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if ext == "lnk" {
        if let Some(exe_name) = get_lnk_target_exe(path) {
            let exe_lower = exe_name.to_lowercase();
            let index = get_exe_index();
            if let Some(&cat) = index.get(&exe_lower) {
                return cat.to_string();
            }
            // 尝试无扩展名匹配
            if let Some(stem) = Path::new(&exe_lower).file_stem() {
                let stem_str = stem.to_string_lossy().to_string();
                // 搜索 exe 名称中包含的关键词
                for (folders, cat, _) in FOLDER_CATEGORIES {
                    for folder in *folders {
                        let folder_lower = folder.to_lowercase();
                        if stem_str.contains(&folder_lower)
                            || folder_lower.contains(&stem_str)
                        {
                            return cat.to_string();
                        }
                    }
                }
            }
        }
        "其他".to_string()
    } else {
        "文件".to_string()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CategorizedItem {
    name: String,
    path: String,
    category: String,
}

/// 获取分类后的桌面文件列表
#[tauri::command]
fn get_categorized_desktop_items() -> Result<Vec<CategorizedItem>, String> {
    let desktop_path = dirs::desktop_dir().ok_or("无法找到桌面目录")?;
    let entries = std::fs::read_dir(desktop_path).map_err(|e| e.to_string())?;
    let mut items = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let category = categorize_desktop_item(&path);

        items.push(CategorizedItem {
            name,
            path: path.to_string_lossy().to_string(),
            category,
        });
    }

    Ok(items)
}

// ============================================================
// 窗口管理器 (Windows API)
// ============================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowInfo {
    /// 窗口句柄（作为整数传到前端）
    hwnd: isize,
    /// 窗口标题
    title: String,
    /// 进程名称 (如 "notepad.exe")
    process_name: String,
    /// 进程完整路径
    process_path: Option<String>,
}

/// 枚举系统所有可见的顶层窗口，排除系统窗口和自身
#[tauri::command]
fn get_system_windows() -> Result<Vec<WindowInfo>, String> {
    let my_pid = std::process::id();
    let windows = enumerate_windows();
    let mut result = Vec::new();

    for hwnd in windows {
        unsafe {
            // 过滤：不可见的窗口跳过
            if IsWindowVisible(hwnd) == BOOL(0) {
                continue;
            }

            // 获取窗口标题
            let mut title_buf = [0u16; 512];
            let title_len = GetWindowTextW(hwnd, &mut title_buf);
            if title_len <= 0 {
                continue;
            }
            let title = String::from_utf16_lossy(&title_buf[..title_len as usize])
                .trim()
                .to_string();
            if title.is_empty() {
                continue;
            }

            // 获取窗口类名，过滤系统窗口
            let mut class_buf = [0u16; 128];
            let class_len = GetClassNameW(hwnd, &mut class_buf);
            if class_len > 0 {
                let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);
                // 排除系统桌面/任务栏窗口
                match class_name.as_str() {
                    "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
                    | "Windows.UI.Core.CoreWindow" | "ApplicationFrameWindow"
                    | "TaskManagerWindow" | "MultitaskingViewFrame" => continue,
                    _ => {}
                }
            }

            // 获取进程 ID
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));

            // 排除自身进程的窗口
            if pid == my_pid {
                continue;
            }

            // 获取进程路径和名称
            let (process_name, process_path) = get_process_info(pid);

            // 无法获取进程信息的窗口跳过（系统进程或无权限）
            let process_name = match process_name {
                Some(name) => name,
                None => continue,
            };

            // 排除 my-desktop-app 自身
            if process_name.eq_ignore_ascii_case("my-desktop-app.exe") {
                continue;
            }

            result.push(WindowInfo {
                hwnd: hwnd.0 as isize,
                title,
                process_name,
                process_path,
            });
        }
    }

    Ok(result)
}

/// 切换到指定窗口（激活/还原）
#[tauri::command]
fn switch_to_window(hwnd: isize) -> Result<(), String> {
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
    Ok(())
}

/// 退出应用
#[tauri::command]
fn exit_app() {
    std::process::exit(0);
}

/// 启动一个应用程序
#[tauri::command]
fn start_app(path: String) -> Result<(), String> {
    Command::new(&path)
        .spawn()
        .map_err(|e| format!("启动失败 ({}): {}", path, e))?;
    Ok(())
}

// ============================================================
// Windows API 辅助函数
// ============================================================

/// 枚举所有顶层窗口，返回 HWND 列表
fn enumerate_windows() -> Vec<HWND> {
    let mut windows = Vec::new();
    let ctx = &mut windows as *mut Vec<HWND>;

    // SAFETY: EnumWindows 是同步的，回调只在函数执行期间被调用
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let windows = &mut *(lparam.0 as *mut Vec<HWND>);
        windows.push(hwnd);
        TRUE
    }

    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(ctx as isize));
    }

    windows
}

/// 根据 PID 获取进程名称和完整路径
fn get_process_info(pid: u32) -> (Option<String>, Option<String>) {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION, false, pid);
        match handle {
            Ok(h) => {
                let mut buf = [0u16; 260]; // MAX_PATH
                let mut size = buf.len() as u32;
                let success = QueryFullProcessImageNameW(
                    h,
                    PROCESS_NAME_WIN32,
                    windows::core::PWSTR(buf.as_mut_ptr()),
                    &mut size as *mut u32,
                )
                .is_ok();

                let _ = CloseHandle(h);

                if success {
                    let path_str =
                        String::from_utf16_lossy(&buf[..size as usize]);
                    let path = std::path::Path::new(&path_str);
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string());
                    (name, Some(path_str))
                } else {
                    (None, None)
                }
            }
            Err(_) => (None, None),
        }
    }
}

// ---- 桌面图标 + 窗口置底 + 拖拽 ----

/// 查找桌面图标列表的 SysListView32 窗口句柄
fn find_desktop_list_view() -> Option<HWND> {
    unsafe {
        // 方法1: 通过 Progman
        let progman = FindWindowW(windows::core::w!("Progman"), None);
        if let Ok(h) = progman {
            let def_view = FindWindowExW(h, None, windows::core::w!("SHELLDLL_DefView"), None);
            if let Ok(dv) = def_view {
                if dv.0 != std::ptr::null_mut() {
                    let list = FindWindowExW(dv, None, windows::core::w!("SysListView32"), None);
                    if let Ok(lv) = list {
                        if lv.0 != std::ptr::null_mut() {
                            return Some(lv);
                        }
                    }
                }
            }
        }
        None
    }
}

#[tauri::command]
fn hide_desktop_icons() -> Result<(), String> {
    if let Some(hwnd) = find_desktop_list_view() {
        unsafe { let _ = ShowWindow(hwnd, SW_HIDE); }
        Ok(())
    } else {
        Err("未找到桌面图标窗口".into())
    }
}

// ---- 虚拟鼠标 ----
#[tauri::command]
fn get_screen_size() -> Result<(i32, i32), String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        Ok((w, h))
    }
}

#[tauri::command]
fn mouse_move_abs(x: i32, y: i32) -> Result<(), String> {
    unsafe { let _ = SetCursorPos(x, y); }
    Ok(())
}

#[tauri::command]
fn mouse_move(dx: i32, dy: i32) -> Result<(), String> {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        let _ = GetCursorPos(&mut pt);
        let _ = SetCursorPos(pt.x + dx, pt.y + dy);
    }
    Ok(())
}

#[tauri::command]
fn mouse_click(button: String) -> Result<(), String> {
    unsafe {
        if button == "right" {
            let _ = mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0);
            let _ = mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0);
        } else {
            let _ = mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
            let _ = mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
        }
    }
    Ok(())
}

#[tauri::command]
fn mouse_down(button: String) -> Result<(), String> {
    unsafe {
        let _ = if button == "right" {
            mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0)
        } else {
            mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0)
        };
    }
    Ok(())
}

#[tauri::command]
fn mouse_up(button: String) -> Result<(), String> {
    unsafe {
        let _ = if button == "right" {
            mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0)
        } else {
            mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0)
        };
    }
    Ok(())
}

/// 将应用窗口自身置底（让其他窗口显示在前面）
#[tauri::command]
fn move_self_to_bottom() -> Result<(), String> {
    let my_pid = std::process::id();
    unsafe {
        let windows = enumerate_windows();
        for hwnd in windows {
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == my_pid {
                let _ = SetWindowPos(
                    hwnd,
                    HWND(1isize as *mut _), // HWND_BOTTOM
                    0, 0, 0, 0,
                    SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE,
                );
                return Ok(());
            }
        }
    }
    Err("未找到自身窗口".into())
}

#[tauri::command]
fn show_desktop_icons() -> Result<(), String> {
    if let Some(hwnd) = find_desktop_list_view() {
        unsafe { let _ = ShowWindow(hwnd, SW_SHOW); }
        Ok(())
    } else {
        Err("未找到桌面图标窗口".into())
    }
}

// ============================================================
// 入口函数
// ============================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // 窗口创建后延时置底（确保 JS 端重试失败时也能生效）
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                let my_pid = std::process::id();
                unsafe {
                    let windows = enumerate_windows();
                    for hwnd in windows {
                        let mut pid: u32 = 0;
                        GetWindowThreadProcessId(hwnd, Some(&mut pid));
                        if pid == my_pid {
                            let _ = SetWindowPos(
                                hwnd,
                                HWND(1isize as *mut _), // HWND_BOTTOM
                                0, 0, 0, 0,
                                SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE,
                            );
                            break;
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_desktop_items,
            start_watcher,
            get_file_icon_base64,
            get_wallpaper_base64,
            get_username,
            get_categorized_desktop_items,
            load_category_config,
            save_category_config,
            get_system_windows,
            switch_to_window,
            start_app,
            exit_app,
            hide_desktop_icons,
            show_desktop_icons,
            move_self_to_bottom,
            mouse_move,
            mouse_move_abs,
            get_screen_size,
            mouse_click,
            mouse_down,
            mouse_up,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
