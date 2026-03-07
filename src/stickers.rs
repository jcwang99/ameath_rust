pub fn get_sticker_bytes(path: &str) -> Option<&'static [u8]> {
    let lower = path.to_lowercase().replace("\\", "/");

    // 只要路径包含表情包名，就返回对应的嵌入字节
    if lower.contains("ok") { return Some(include_bytes!("../assets/stickers/OK.gif")); }
    if lower.contains("不ok") { return Some(include_bytes!("../assets/stickers/不OK.gif")); }
    if lower.contains("写笔记") { return Some(include_bytes!("../assets/stickers/写笔记.gif")); }
    if lower.contains("加班") { return Some(include_bytes!("../assets/stickers/加班.gif")); }
    if lower.contains("发呆") { return Some(include_bytes!("../assets/stickers/发呆.gif")); }
    if lower.contains("吃瓜") { return Some(include_bytes!("../assets/stickers/吃瓜.gif")); }
    if lower.contains("喵喵") { return Some(include_bytes!("../assets/stickers/喵喵.gif")); }
    if lower.contains("嘲笑") { return Some(include_bytes!("../assets/stickers/嘲笑.gif")); }
    if lower.contains("打你") { return Some(include_bytes!("../assets/stickers/打你.gif")); }
    if lower.contains("扯脸") { return Some(include_bytes!("../assets/stickers/扯脸.gif")); }
    if lower.contains("探头") { return Some(include_bytes!("../assets/stickers/探头.gif")); }
    if lower.contains("星星眼") { return Some(include_bytes!("../assets/stickers/星星眼.gif")); }
    if lower.contains("比心") { return Some(include_bytes!("../assets/stickers/比心.gif")); }
    if lower.contains("生气") { return Some(include_bytes!("../assets/stickers/生气.gif")); }
    if lower.contains("睡觉") { return Some(include_bytes!("../assets/stickers/睡觉.gif")); }
    if lower.contains("给玫瑰") { return Some(include_bytes!("../assets/stickers/给玫瑰.gif")); }
    if lower.contains("脸红") { return Some(include_bytes!("../assets/stickers/脸红.gif")); }
    if lower.contains("被摸头") { return Some(include_bytes!("../assets/stickers/被摸头.gif")); }
    if lower.contains("贴贴") { return Some(include_bytes!("../assets/stickers/贴贴.gif")); }
    if lower.contains("饿饿") { return Some(include_bytes!("../assets/stickers/饿饿.gif")); }

    None
}
