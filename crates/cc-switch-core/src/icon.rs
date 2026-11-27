//! Custom icon value validation, independent of persistence and UI.
use crate::error::AppError;
use base64::Engine;
use std::io::Cursor;

/// 自定义图标的前缀，和前端 `src/utils/customIcon.ts` 的 `CUSTOM_ICON_PREFIX` 一致。
pub const CUSTOM_ICON_PREFIX: &str = "data:image/png;base64,";

/// 整个 data URI 的长度上限，和前端的 `MAX_CUSTOM_ICON_LENGTH` 一致。
/// 前端输出 96×96 的 PNG，最坏情况（不可压缩）base64 后约 50 KB。
pub const MAX_CUSTOM_ICON_LENGTH: usize = 64 * 1024;

/// PNG 宽高上限，和前端的 `MAX_CUSTOM_ICON_DIMENSION` 一致。前端只输出 96×96，留出余量即可。
const MAX_CUSTOM_ICON_DIMENSION: u32 = 256;

/// 解码时允许分配的内存：256×256 的 16 位 RGBA 是 512 KiB，留一倍余量
const DECODE_LIMIT_BYTES: usize = 1024 * 1024;

/// 是不是自定义图标（任何 `data:` 开头的值都算，交给 [`validate_icon`] 判断是否合法）。
pub fn is_custom_icon(icon: &str) -> bool {
    icon.starts_with("data:")
}

/// 保存供应商前校验 `icon`。
///
/// 只管 `data:` 开头的值；其余字符串照旧当图标名放行（找不到对应内置图标时前端显示首字母），
/// 否则库里已有的旧值会让用户连编辑都保存不了。
pub fn validate_icon(icon: Option<&str>) -> Result<(), AppError> {
    match icon {
        Some(icon) if is_custom_icon(icon) && !is_valid_custom_icon(icon) => {
            Err(AppError::localized(
                "provider.icon.invalid",
                "自定义图标无效，请重新上传",
                "The custom icon is invalid, please upload it again",
            ))
        }
        _ => Ok(()),
    }
}

/// 完整解码一遍：签名、块校验和、像素数据都要完好，宽高在上限内，解码内存有上限。
pub(crate) fn is_valid_custom_icon(icon: &str) -> bool {
    if icon.len() > MAX_CUSTOM_ICON_LENGTH {
        return false;
    }
    let Some(encoded) = icon.strip_prefix(CUSTOM_ICON_PREFIX) else {
        return false;
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
        return false;
    };
    let decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes.as_slice()),
        png::Limits {
            bytes: DECODE_LIMIT_BYTES,
        },
    );
    let Ok(mut reader) = decoder.read_info() else {
        return false;
    };
    let (width, height) = reader.info().size();
    // 先看宽高再分配像素缓冲区
    if width == 0
        || height == 0
        || width > MAX_CUSTOM_ICON_DIMENSION
        || height > MAX_CUSTOM_ICON_DIMENSION
    {
        return false;
    }
    let Some(size) = reader.output_buffer_size() else {
        return false;
    };
    let mut buf = vec![0; size];
    reader.next_frame(&mut buf).is_ok() && reader.finish().is_ok()
}
