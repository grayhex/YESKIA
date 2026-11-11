// Narrow read-only AppArc bridge. Every Symbian leave remains inside a TRAP boundary.
// No launch, install, remove or PIM mutation symbols are exposed by this bridge.
#include <e32base.h>
#include <apgcli.h>
#include <apadef.h>
#include <stdint.h>
struct YeskiaAppRow { uint32_t uid; int32_t caption_len; uint16_t caption[64]; int32_t truncated; };
static void ReadPageL(TInt offset, TInt cap, YeskiaAppRow* out, TInt* count, TInt* more) {
    RApaLsSession session;
    User::LeaveIfError(session.Connect());
    CleanupClosePushL(session);
    User::LeaveIfError(session.GetAllApps());
    TApaAppInfo info;
    TInt index = 0;
    TInt result;
    while ((result = session.GetNextApp(info)) == KErrNone) {
        if (index++ < offset) continue;
        if (*count == cap) { *more = 1; break; }
        YeskiaAppRow& row = out[(*count)++];
        row.uid = (uint32_t)info.iUid.iUid;
        row.caption_len = Min(info.iCaption.Length(), 64);
        row.truncated = info.iCaption.Length() > 64;
        for (TInt i=0; i<row.caption_len; ++i) row.caption[i] = info.iCaption[i];
        if (index > 4096) User::Leave(KErrOverflow);
    }
    // RApaLsSession uses positive ENoMoreAppsInList (1), not KErrNotFound.
    // EAppListInvalid is also positive: LeaveIfError alone would hide that failure.
    if (*more == 0 && result != RApaLsSession::ENoMoreAppsInList)
        User::Leave(result < 0 ? result : KErrNotReady);
    CleanupStack::PopAndDestroy(&session);
}
extern "C" int32_t yeskia_apps_page(int32_t offset, int32_t cap, YeskiaAppRow* out, int32_t* count, int32_t* more) {
    if (!out || !count || !more || offset < 0 || offset > 4096 || cap < 1 || cap > 32) return KErrArgument;
    *count = 0; *more = 0;
    TRAPD(err, ReadPageL(offset, cap, out, count, more));
    return err;
}
