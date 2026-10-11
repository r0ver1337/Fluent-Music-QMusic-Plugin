use crate::{arena::Arena, decode, types::*};
use fluent_plugin_api::*;

impl Arena {
    pub(crate) fn qualityList(&mut self, value: &QualityList) -> fmQualityList {
        let levels = value
            .Levels
            .iter()
            .map(|level| fmQualityLevel {
                priority: level.Priority,
                name: self.text(&level.Name),
                disabled: level.Disabled as u32,
                ..fmQualityLevel::new()
            })
            .collect();
        let items = value
            .Items
            .iter()
            .map(|item| fmTrackQuality {
                id: self.text(&item.Id),
                name: self.text(&item.Name),
                priority: item.Priority,
                hasFileSize: item.FileSizeBytes.is_some() as u32,
                fileSizeBytes: item.FileSizeBytes.unwrap_or_default(),
                playable: item.Playable as u32,
                unavailableReason: self.optional(&item.UnavailableReason),
                ..fmTrackQuality::new()
            })
            .collect();
        fmQualityList {
            levels: self.array(levels),
            items: self.array(items),
            ..fmQualityList::new()
        }
    }
}

pub(crate) unsafe fn decodeQualityList(
    pointer: *const std::ffi::c_void,
) -> Result<QualityList, ApiError> {
    let value: fmQualityList = unsafe { decode::body(pointer)? };
    if value.levels.count > maxQualityEntries as u64 || value.items.count > maxQualityEntries as u64
    {
        return Err(decode::invalid());
    }
    let levels = unsafe { decodeLevels(value.levels)? };
    let mut items = Vec::new();
    for item in unsafe { decode::array::<fmTrackQuality>(value.items)? } {
        if item.structSize < size_of::<fmTrackQuality>() as u32
            || item.hasFileSize > 1
            || item.playable > 1
            || (item.hasFileSize == 0 && item.fileSizeBytes != 0)
        {
            return Err(decode::invalid());
        }
        let reason = unsafe { decode::text(item.unavailableReason)? };
        items.push(TrackQuality {
            Id: unsafe { decode::text(item.id)? },
            Name: unsafe { decode::text(item.name)? },
            Priority: item.priority,
            FileSizeBytes: (item.hasFileSize == 1).then_some(item.fileSizeBytes),
            Playable: item.playable == 1,
            UnavailableReason: (!reason.is_empty()).then_some(reason),
        });
    }
    let list = QualityList {
        Levels: levels,
        Items: items,
    };
    list.validate()?;
    Ok(list)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LevelPrefix {
    StructSize: u32,
    Priority: u32,
    Name: fmStringView,
}

unsafe fn decodeLevels(slice: fmSlice) -> Result<Vec<QualityLevel>, ApiError> {
    let mut levels = Vec::new();
    // Older v1 records end before the optional disabled flag.
    for (index, level) in unsafe { decode::array::<LevelPrefix>(slice)? }
        .into_iter()
        .enumerate()
    {
        if level.StructSize < size_of::<LevelPrefix>() as u32
            || u64::from(level.StructSize) > slice.stride
        {
            return Err(decode::invalid());
        }
        let offset = std::mem::offset_of!(fmQualityLevel, disabled);
        let disabled = if level.StructSize as usize >= offset + size_of::<u32>() {
            decode::consume(size_of::<u32>() as u64)?;
            unsafe {
                slice
                    .data
                    .cast::<u8>()
                    .add(index * slice.stride as usize + offset)
                    .cast::<u32>()
                    .read()
            }
        } else {
            0
        };
        if disabled > 1 {
            return Err(decode::invalid());
        }
        levels.push(QualityLevel {
            Priority: level.Priority,
            Name: unsafe { decode::text(level.Name)? },
            Disabled: disabled == 1,
        });
    }
    Ok(levels)
}
