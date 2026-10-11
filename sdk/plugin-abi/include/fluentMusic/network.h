#ifndef fluentMusicNetworkH
#define fluentMusicNetworkH
#include "types.h"
typedef struct fmNetworkApi {
    uint32_t structSize;
    void *context;
    fmStatus (fmCallConvention *call)(void *context, const fmStringView *requestJson, fmHandle *outReply);
    const fmStringView *(fmCallConvention *getReply)(fmHandle reply);
    void (fmCallConvention *releaseReply)(fmHandle reply);
} fmNetworkApi;
#endif
