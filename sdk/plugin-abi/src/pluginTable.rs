use crate::types::*;
use std::mem::offset_of;

/// # Safety
/// `pointer` must reference the readable table prefix advertised by `structSize`.
pub unsafe fn readPluginApi(pointer: *const fmPluginApi) -> Result<fmPluginApi, AbiFault> {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<fmPluginApi>()) {
        return Err(AbiFault);
    }
    let size = unsafe { pointer.cast::<u32>().read() };
    if size < offset_of!(fmPluginApi, getLyrics) as u32 {
        return Err(AbiFault);
    }
    macro_rules! field {
        ($name:ident, $ty:ty) => {
            unsafe {
                pointer
                    .cast::<u8>()
                    .add(offset_of!(fmPluginApi, $name))
                    .cast::<$ty>()
                    .read()
            }
        };
    }
    if field!(abiVersion, u32) != abiVersion {
        return Err(AbiFault);
    }
    macro_rules! optional {
        ($name:ident) => {
            if size as usize >= offset_of!(fmPluginApi, $name) + size_of::<Option<fmCall>>() {
                field!($name, Option<fmCall>)
            } else {
                None
            }
        };
    }
    Ok(fmPluginApi {
        structSize: size,
        abiVersion,
        createProvider: field!(
            createProvider,
            Option<
                unsafe extern "C" fn(
                    *const fmHostApi,
                    *const fmCreateInfo,
                    *mut fmHandle,
                ) -> fmStatus,
            >
        )
        .ok_or(AbiFault)?,
        destroyProvider: field!(
            destroyProvider,
            Option<unsafe extern "C" fn(fmHandle) -> fmStatus>
        )
        .ok_or(AbiFault)?,
        describe: Some(field!(describe, Option<fmCall>).ok_or(AbiFault)?),
        loginBegin: field!(loginBegin, Option<fmCall>),
        loginPoll: field!(loginPoll, Option<fmCall>),
        logout: field!(logout, Option<fmCall>),
        getAccount: field!(getAccount, Option<fmCall>),
        search: field!(search, Option<fmCall>),
        listPlaylists: field!(listPlaylists, Option<fmCall>),
        getPlaylist: field!(getPlaylist, Option<fmCall>),
        getRecommendations: field!(getRecommendations, Option<fmCall>),
        getHomepage: field!(getHomepage, Option<fmCall>),
        getPlaylistRecommendations: field!(getPlaylistRecommendations, Option<fmCall>),
        resolveTrack: field!(resolveTrack, Option<fmCall>),
        getResultView: field!(
            getResultView,
            Option<unsafe extern "C" fn(fmHandle) -> *const fmResultView>
        )
        .ok_or(AbiFault)?,
        releaseResult: field!(releaseResult, Option<unsafe extern "C" fn(fmHandle)>)
            .ok_or(AbiFault)?,
        getLyrics: optional!(getLyrics),
        getTrackQualities: optional!(getTrackQualities),
        resolveTrackWithQuality: optional!(resolveTrackWithQuality),
    })
}
