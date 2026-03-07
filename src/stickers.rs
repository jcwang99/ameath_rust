pub fn get_sticker_bytes(path: &str) -> Option<&'static [u8]> {
    // 将路径统一转换为小写并清理分隔符
    let clean_path = path.to_lowercase().replace("\\", "/");
    
    // 我们只需要判断路径中是否包含特定的关键字（表情包的文件名部分）
    // 这样即便 AI 发送的是 assets/stickers/喵喵.gif 或 喵喵.gif 都能匹配成功
    if clean_path.contains("ok.gif") { return Some(include_bytes!("../assets/stickers/OK.gif")); }
    if clean_path.contains("不ok.gif") { return Some(include_bytes!("../assets/stickers/不OK.gif")); }
    if clean_path.contains("写笔记.gif") { return Some(include_bytes!("../assets/stickers/写笔记.gif")); }
    if clean_path.contains("加班.gif") { return Some(include_bytes!("../assets/stickers/加班.gif")); }
    if clean_path.contains("发呆.gif") { return Some(include_bytes!("../assets/stickers/发呆.gif")); }
    if clean_path.contains("吃瓜.gif") { return Some(include_bytes!("../assets/stickers/吃瓜.gif")); }
    if clean_path.contains("喵喵.gif") { return Some(include_bytes!("../assets/stickers/喵喵.gif")); }
    if clean_path.contains("嘲笑.gif") { return Some(include_bytes!("../assets/stickers/嘲笑.gif")); }
    if clean_path.contains("打你.gif") { return Some(include_bytes!("../assets/stickers/打你.gif")); }
    if clean_path.contains("扯脸.gif") { return Some(include_bytes!("../assets/stickers/扯脸.gif")); }
    if clean_path.contains("探头.gif") { return Some(include_bytes!("../assets/stickers/探头.gif")); }
    if clean_path.contains("星星眼.gif") { return Some(include_bytes!("../assets/stickers/星星眼.gif")); }
    if clean_path.contains("比心.gif") { return Some(include_bytes!("../assets/stickers/比心.gif")); }
    if clean_path.contains("生气.gif") { return Some(include_bytes!("../assets/stickers/生气.gif")); }
    if clean_path.contains("睡觉.gif") { return Some(include_bytes!("../assets/stickers/睡觉.gif")); }
    if clean_path.contains("给玫瑰.gif") { return Some(include_bytes!("../assets/stickers/给玫瑰.gif")); }
    if clean_path.contains("脸红.gif") { return Some(include_bytes!("../assets/stickers/脸红.gif")); }
    if clean_path.contains("被摸头.gif") { return Some(include_bytes!("../assets/stickers/被摸头.gif")); }
    if clean_path.contains("贴贴.gif") { return Some(include_bytes!("../assets/stickers/贴贴.gif")); }
    if clean_path.contains("饿饿.gif") { return Some(include_bytes!("../assets/stickers/饿饿.gif")); }

    None
}
