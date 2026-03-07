pub fn get_sticker_bytes(path: &str) -> Option<&'static [u8]> {
    let clean_path = path.replace("\\", "/");
    let filename = if clean_path.contains("/") {
        clean_path.split('/').last().unwrap_or("")
    } else {
        &clean_path
    };

    match filename {
        "OK.gif" => Some(include_bytes!("../assets/stickers/OK.gif")),
        "不OK.gif" => Some(include_bytes!("../assets/stickers/不OK.gif")),
        "写笔记.gif" => Some(include_bytes!("../assets/stickers/写笔记.gif")),
        "加班.gif" => Some(include_bytes!("../assets/stickers/加班.gif")),
        "发呆.gif" => Some(include_bytes!("../assets/stickers/发呆.gif")),
        "吃瓜.gif" => Some(include_bytes!("../assets/stickers/吃瓜.gif")),
        "喵喵.gif" => Some(include_bytes!("../assets/stickers/喵喵.gif")),
        "嘲笑.gif" => Some(include_bytes!("../assets/stickers/嘲笑.gif")),
        "打你.gif" => Some(include_bytes!("../assets/stickers/打你.gif")),
        "扯脸.gif" => Some(include_bytes!("../assets/stickers/扯脸.gif")),
        "探头.gif" => Some(include_bytes!("../assets/stickers/探头.gif")),
        "星星眼.gif" => Some(include_bytes!("../assets/stickers/星星眼.gif")),
        "比心.gif" => Some(include_bytes!("../assets/stickers/比心.gif")),
        "生气.gif" => Some(include_bytes!("../assets/stickers/生气.gif")),
        "睡觉.gif" => Some(include_bytes!("../assets/stickers/睡觉.gif")),
        "给玫瑰.gif" => Some(include_bytes!("../assets/stickers/给玫瑰.gif")),
        "脸红.gif" => Some(include_bytes!("../assets/stickers/脸红.gif")),
        "被摸头.gif" => Some(include_bytes!("../assets/stickers/被摸头.gif")),
        "贴贴.gif" => Some(include_bytes!("../assets/stickers/贴贴.gif")),
        "饿饿.gif" => Some(include_bytes!("../assets/stickers/饿饿.gif")),
        _ => None,
    }
}
