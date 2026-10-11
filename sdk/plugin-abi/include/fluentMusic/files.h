#ifndef fluentMusicFilesH
#define fluentMusicFilesH
#include "types.h"
typedef struct fmFileRequest {
    uint32_t structSize, operation;
    uint64_t handle, targetHandle;
    uint32_t flags;
    uint64_t offset, length;
    fmStringView path, targetPath, bytes;
} fmFileRequest;
typedef struct fmFileEntry { fmStringView name; uint32_t kind; uint64_t length; } fmFileEntry;
typedef struct fmFileReply { uint32_t structSize; int32_t status; uint64_t handle, length; uint32_t kind; fmStringView bytes; fmSlice entries; } fmFileReply;
typedef struct fmFileApi {
    uint32_t structSize;
    void *context;
    fmStatus (fmCallConvention *call)(void *context, const fmFileRequest *request, fmHandle *outReply);
    const fmFileReply *(fmCallConvention *getReply)(fmHandle reply);
    void (fmCallConvention *releaseReply)(fmHandle reply);
} fmFileApi;
enum { fmRootPlugin = 1, fmRootData = 2, fmRootTemp = 3 };
enum { fmOpenRead = 1, fmOpenWrite = 2, fmOpenCreate = 4, fmOpenTruncate = 8 };
enum {
    fmGetRoot = 0, fmOpenDirectory = 1, fmCreateDirectory = 2, fmOpenFile = 3,
    fmReadAt = 4, fmWriteAt = 5, fmGetFileInfo = 6, fmSetFileSize = 7,
    fmFlushFile = 8, fmRenameEntry = 9, fmAtomicReplace = 10, fmRemoveEntry = 11,
    fmCloseFile = 12, fmCloseDirectory = 13, fmCreateMediaOutput = 14,
    fmFinishMediaOutput = 15, fmRetainMedia = 16, fmReleaseMedia = 17, fmTryLockFile = 18,
    fmListDirectory = 19, fmRequestFileAccess = 20, fmRequestDirectoryAccess = 21
};
enum { fmFileOk = 0, fmFileDenied = 1, fmFileNotFound = 2, fmFileInvalid = 3, fmFileIoError = 4, fmFileBusy = 5 };
#endif
