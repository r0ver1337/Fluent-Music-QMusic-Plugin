use super::*;
use fluent_plugin_api::*;

#[repr(C)]
#[derive(Clone, Copy)]
struct OldLevel {
    StructSize: u32,
    Priority: u32,
    Name: fmStringView,
}

#[test]
fn exactOldLevelAllocationUsesEnabledDefaultWithoutReadingTheTail() {
    let mut arena = arena::Arena::default();
    let name = arena.text("无损");
    let levels = arena.array(vec![OldLevel {
        StructSize: size_of::<OldLevel>() as u32,
        Priority: 1,
        Name: name,
    }]);
    let list = fmQualityList {
        levels,
        ..fmQualityList::new()
    };
    let view = fmResultView {
        kind: 11,
        data: (&list as *const fmQualityList).cast(),
        ..fmResultView::new()
    };
    let ResultData::TrackQualities(decoded) = unsafe { decodeResult(&view) }.unwrap() else {
        panic!("list")
    };
    assert!(!decoded.Levels[0].Disabled);
    assert_eq!(decoded.Levels[0].Name, "无损");
}

#[test]
fn disabledTailIsTypedAndInvalidFlagsAndTruncatedStridesAreRejected() {
    let mut arena = arena::Arena::default();
    let name = arena.text("杜比");
    for (disabled, stride) in [
        (1, size_of::<fmQualityLevel>() as u64),
        (2, size_of::<fmQualityLevel>() as u64),
        (1, size_of::<OldLevel>() as u64),
    ] {
        let mut levels = arena.array(vec![fmQualityLevel {
            priority: 1,
            name,
            disabled,
            ..fmQualityLevel::new()
        }]);
        levels.stride = stride;
        let list = fmQualityList {
            levels,
            ..fmQualityList::new()
        };
        let view = fmResultView {
            kind: 11,
            data: (&list as *const fmQualityList).cast(),
            ..fmResultView::new()
        };
        let decoded = unsafe { decodeResult(&view) };
        if disabled == 1 && stride == size_of::<fmQualityLevel>() as u64 {
            let ResultData::TrackQualities(decoded) = decoded.unwrap() else {
                panic!("list")
            };
            assert!(decoded.Levels[0].Disabled);
        } else {
            assert!(decoded.is_err());
        }
    }
}
