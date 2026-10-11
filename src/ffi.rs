use fluent_plugin_abi::*;
use fluent_plugin_api::ApiError;
use std::sync::Mutex;

unsafe extern "C" fn create_provider(
    host: *const fmHostApi,
    info: *const fmCreateInfo,
    out: *mut fmHandle,
) -> fmStatus {
    if info.is_null() || out.is_null() {
        return -1;
    }
    unsafe {
        *out = std::ptr::null_mut();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if unsafe { (*info).structSize } < size_of::<fmCreateInfo>() as u32 {
            return Err(());
        }
        unsafe {
            fluent_plugin_abi::io::installApi(host).map_err(|_| ())?;
        }
        let provider = crate::provider::Provider::persistent();
        Ok::<_, ()>(provider)
    }));
    match result {
        Ok(Ok(provider)) => {
            unsafe {
                *out = Box::into_raw(Box::new(Mutex::new(provider))).cast();
            }
            0
        }
        _ => -1,
    }
}
unsafe extern "C" fn destroy_provider(handle: fmHandle) -> fmStatus {
    if handle.is_null() {
        return -1;
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        drop(Box::from_raw(
            handle.cast::<Mutex<crate::provider::Provider>>(),
        ));
    })) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}
unsafe extern "C" fn call(
    handle: fmHandle,
    context: *const fmCallContext,
    request: *const fmRequest,
    out: *mut fmHandle,
) -> fmStatus {
    if handle.is_null() || out.is_null() || context.is_null() {
        return -1;
    }
    unsafe {
        *out = std::ptr::null_mut();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if unsafe { (*context).structSize } < size_of::<fmCallContext>() as u32 {
            return Err(ApiError::new("ABI", "调用上下文无效"));
        }
        if unsafe { (*context).isCancelled }.is_some_and(|f| unsafe { f((*context).userData) } != 0)
        {
            return Err(ApiError::new("CANCELLED", "请求已取消"));
        }
        let operation = unsafe { decodeRequest(request)? };
        let mut provider = unsafe { &*handle.cast::<Mutex<crate::provider::Provider>>() }
            .lock()
            .map_err(|_| ApiError::new("ABI", "插件实例已失效"))?;
        provider.handle(operation)
    }));
    let result = result.unwrap_or_else(|_| Err(ApiError::new("ABI", "插件发生内部错误")));
    unsafe {
        *out = OwnedResult::new(result).intoHandle();
    }
    0
}
unsafe extern "C" fn get_result_view(handle: fmHandle) -> *const fmResultView {
    if handle.is_null() {
        return std::ptr::null();
    }
    unsafe { &(*handle.cast::<OwnedResult>()).View }
}
unsafe extern "C" fn release_result(handle: fmHandle) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle.cast::<OwnedResult>()));
        }
    }
}
static PLUGIN_API: fmPluginApi = fmPluginApi {
    structSize: size_of::<fmPluginApi>() as u32,
    abiVersion,
    createProvider: create_provider,
    destroyProvider: destroy_provider,
    describe: Some(call),
    loginBegin: Some(call),
    loginPoll: Some(call),
    logout: Some(call),
    getAccount: Some(call),
    search: Some(call),
    listPlaylists: Some(call),
    getPlaylist: Some(call),
    getRecommendations: Some(call),
    getHomepage: Some(call),
    getPlaylistRecommendations: Some(call),
    resolveTrack: Some(call),
    getResultView: get_result_view,
    releaseResult: release_result,
    getLyrics: Some(call),
    getTrackQualities: Some(call),
    resolveTrackWithQuality: Some(call),
};

#[unsafe(no_mangle)]
#[allow(non_snake_case)] // The plugin loader requires this exact exported ABI symbol.
/// # Safety
/// `out` must point to writable pointer storage owned by the caller.
pub unsafe extern "C" fn fmPluginGetApi(version: u32, out: *mut *const fmPluginApi) -> fmStatus {
    if out.is_null() {
        return -1;
    }
    unsafe {
        *out = std::ptr::null();
    }
    if version != abiVersion {
        return -2;
    }
    unsafe {
        *out = &PLUGIN_API;
    }
    0
}
