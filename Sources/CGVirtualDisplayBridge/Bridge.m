#import "CGVirtualDisplay.h"
#import <dlfcn.h>
#import <stdbool.h>

// 设旋转只能运行时按类名找 MPDisplay：直接引用类符号会产生 _OBJC_CLASS_$_MPDisplay 的链接期引用，
// 而 MonitorPanel 私有框架不在链接集合里。类不在（系统改名/移除）就返回 1，调用方只记日志、不当回事
NSInteger SnugSetDisplayRotation(CGDirectDisplayID displayID, NSInteger degrees) {
    Class cls = NSClassFromString(@"MPDisplay");
    if (cls == Nil) {
        [[NSBundle bundleWithPath:@"/System/Library/PrivateFrameworks/MonitorPanel.framework"] load];
        cls = NSClassFromString(@"MPDisplay");
    }
    if (cls == Nil) { return 1; }
    MPDisplay *display = [[cls alloc] initWithDisplayID:displayID];
    if (display == nil) { return 1; }
    [display setOrientation:degrees];
    // 旋转不是瞬时生效：轮询 CGDisplayRotation，最多等 10 秒（displayplacer 同款）
    for (int i = 0; i < 100 && CGDisplayRotation(displayID) != (double)degrees; i++) {
        [NSThread sleepForTimeInterval:0.1];
    }
    return CGDisplayRotation(displayID) == (double)degrees ? 0 : 2;
}

// 停用/启用一整块屏：SkyLight 的 CGSConfigureDisplayEnabled（连接 + 屏编号 + 开关）。
// 符号只解析一次并缓存；框架或任一符号不在就返回 1，调用方如实记日志、不再重试
typedef uint32_t SnugCGSConnection;
typedef CGError (*SnugCGSConfigureDisplayEnabledFn)(SnugCGSConnection, CGDirectDisplayID, bool);
typedef SnugCGSConnection (*SnugCGSConnectionFn)(void);

NSInteger SnugSetDisplayEnabled(CGDirectDisplayID displayID, BOOL enabled) {
    static SnugCGSConfigureDisplayEnabledFn configureFn = NULL;
    static SnugCGSConnectionFn connectionFn = NULL;
    static BOOL unavailable = NO;
    if (unavailable) { return 1; }
    if (configureFn == NULL) {
        void *skyLight = dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_LAZY);
        if (skyLight == NULL) { unavailable = YES; return 1; }
        configureFn = (SnugCGSConfigureDisplayEnabledFn)dlsym(skyLight, "CGSConfigureDisplayEnabled");
        connectionFn = (SnugCGSConnectionFn)dlsym(skyLight, "CGSDefaultConnectionForThread");
        if (connectionFn == NULL) {
            connectionFn = (SnugCGSConnectionFn)dlsym(skyLight, "_CGSDefaultConnection");
        }
        if (configureFn == NULL || connectionFn == NULL) { unavailable = YES; return 1; }
    }
    CGError err = configureFn(connectionFn(), displayID, enabled ? true : false);
    return err == kCGErrorSuccess ? 0 : 2;
}
