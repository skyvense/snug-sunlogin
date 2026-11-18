#import "CGVirtualDisplay.h"

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
