//  CGVirtualDisplay 私有 API 声明（macOS 14+）
//  来源：FreeDisplay (MIT) https://github.com/huberdf/FreeDisplay ，属性名与 Chromium virtual_display_mac_util.mm 一致
//  补了可空与泛型标注：Swift 侧 initWithDescriptor 返回可选值（同序列号残留屏还在时会返回 nil），modes 有元素类型

#import <Foundation/Foundation.h>
#import <CoreGraphics/CoreGraphics.h>

NS_ASSUME_NONNULL_BEGIN

@interface CGVirtualDisplayDescriptor : NSObject
@property (nonatomic) CGSize   sizeInMillimeters;
@property (nonatomic) uint32_t maxPixelsWide;
@property (nonatomic) uint32_t maxPixelsHigh;
@property (nonatomic) NSPoint  whitePoint;
@property (nonatomic) NSPoint  redPrimary;
@property (nonatomic) NSPoint  greenPrimary;
@property (nonatomic) NSPoint  bluePrimary;
@property (nonatomic, nullable) dispatch_queue_t queue;
@property (nonatomic, copy) NSString *name;
@property (nonatomic) uint32_t vendorID;
@property (nonatomic) uint32_t productID;
@property (nonatomic) uint32_t serialNum;
@end

@interface CGVirtualDisplayMode : NSObject
- (instancetype)initWithWidth:(NSUInteger)width height:(NSUInteger)height refreshRate:(double)refreshRate;
@property (nonatomic, readonly) NSUInteger width;
@property (nonatomic, readonly) NSUInteger height;
@property (nonatomic, readonly) double     refreshRate;
@end

@interface CGVirtualDisplaySettings : NSObject
@property (nonatomic) BOOL hiDPI;
@property (nonatomic, copy) NSArray<CGVirtualDisplayMode *> *modes;
@end

@interface CGVirtualDisplay : NSObject
- (nullable instancetype)initWithDescriptor:(CGVirtualDisplayDescriptor *)descriptor;
- (BOOL)applySettings:(CGVirtualDisplaySettings *)settings;
@property (nonatomic, readonly) CGDirectDisplayID displayID;
@end

//  MPDisplay 私有 API 声明（MonitorPanel 私有框架）：设置显示器旋转——公开 API 只有读（CGDisplayRotation），没有写
//  来源：displayplacer (MIT) https://github.com/jakehilborn/displayplacer 的 src/MonitorPanel.m 同款用法：
//  initWithDisplayID 拿到这块屏，setOrientation 传角度（0/90/180/270），生效要轮询 CGDisplayRotation 等
//  框架不在链接期引入：运行时先 NSClassFromString 找类，找不到再按路径加载；类不在就当这条机制不可用
@interface MPDisplay : NSObject
- (nullable instancetype)initWithDisplayID:(CGDirectDisplayID)displayID;
- (void)setOrientation:(NSInteger)orientation;
@end

NS_ASSUME_NONNULL_END
