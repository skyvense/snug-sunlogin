import CoreGraphics
import Testing
@testable import SnugCore

@Suite("屏幕适配 ScreenFit")
struct ScreenFitTests {
    /// 基准宽度下的可放区域：1173 扣掉菜单栏 25、小横条 28
    private let baseArea = CGSize(width: 540, height: 1120)
    private let fit = ScreenFit.standard

    @Test("没选过手机（iPhone 17 Pro Max）：全屏比例 540×1173，尺寸吸附到 8 的倍数（544×1176），档位 540 到 1080、每 10 点一档、比例都一样")
    func standard() {
        #expect(fit.size(width: 540) == CGSize(width: 544, height: 1176))
        #expect(fit.size(width: 1080) == CGSize(width: 1080, height: 2344))
        #expect(fit.widths.first == 540)
        #expect(fit.widths.last == 1080)
        #expect(zip(fit.widths, fit.widths.dropFirst()).allSatisfy { $1 - $0 == 10 })
        for width in fit.widths {
            let size = fit.size(width: width)
            #expect(abs(size.height / size.width - 1173.0 / 540) < 0.015)
        }
    }

    @Test("像素上限框住所有比例：最宽一档 × 最长比例的 2 倍")
    func maxPixels() {
        #expect(ScreenFit.maxPixels == CGSize(width: 2160, height: 4864))
        let longest = ScreenFit(ratio: 2.25)
        #expect(longest.size(width: 1080).height * 2 == ScreenFit.maxPixels.height)
    }

    @Test("比例 = 手机屏幕的完整高宽比（向日葵 全屏显示、不扣工具栏）")
    func ratio() {
        // 20:9 安卓 1080×2400：2.222
        let android = ScreenFit(phone: PhoneScreen(width: 1080, height: 2400))
        #expect(abs(android.ratio - 2400.0 / 1080) < 1e-9)
        #expect(android.baseWidth == 540)
        #expect(android.size(width: 540) == CGSize(width: 544, height: 1200))
        // 只看比例：自己填 9×20 和 1080×2400 一样
        #expect(ScreenFit(phone: PhoneScreen(width: 9, height: 20)) == android)
    }

    @Test("矮的屏（偏方的折叠屏内屏比例）：540 宽时高不到 800、没有 2 倍，基准加宽到够高的一档")
    func shortScreen() {
        let squarish = ScreenFit(phone: PhoneScreen(width: 1812, height: 2176))
        #expect(squarish.baseWidth == 670)
        #expect(squarish.size(width: 670).height >= 800)
        #expect(squarish.size(width: 660).height < 800)
        #expect(squarish.widths.first == 670)
        #expect(squarish.widths.last == 1080)
    }

    @Test("比例夹在 1 到 2.25：比正方形还方按正方形（基准 800×800）；太长的按 2.25")
    func clamped() {
        let fold = ScreenFit(phone: PhoneScreen(width: 2160, height: 1856))
        #expect(fold.ratio == 1)
        #expect(fold.size(width: fold.baseWidth) == CGSize(width: 800, height: 800))
        let tall = ScreenFit(phone: PhoneScreen(width: 9, height: 30))
        #expect(tall.ratio == 2.25)
        #expect(tall.size(width: 540) == CGSize(width: 544, height: 1216))
    }

    @Test("任何比例的每一档都过 2 倍门槛（宽 ≥ 540、高 ≥ 800），也都在像素上限里")
    func everyStepIsHiDPI() {
        for r in stride(from: 0.8, through: 2.4, by: 0.01) {
            let f = ScreenFit(ratio: r)
            #expect(!f.widths.isEmpty)
            for width in f.widths {
                let size = f.size(width: width)
                #expect(size.width >= CGFloat(ScreenFit.minWidth) && size.height >= CGFloat(ScreenFit.minHeight))
                #expect(size.width * 2 <= ScreenFit.maxPixels.width && size.height * 2 <= ScreenFit.maxPixels.height)
            }
        }
    }

    @Test("装得下就不换（App 取整差一两点也算装得下）")
    func fits() {
        #expect(fit.width(toFit: baseArea, in: baseArea, at: 540) == 540)
        #expect(fit.width(toFit: CGSize(width: 400, height: 600), in: baseArea, at: 540) == 540)
        #expect(fit.width(toFit: CGSize(width: 542, height: 1122), in: baseArea, at: 540) == 540)
    }

    @Test("宽度超了：装得下的最窄一档（系统设置 740、Safari 574、Typeless 988）")
    func wider() {
        #expect(fit.width(toFit: CGSize(width: 740, height: 1120), in: baseArea, at: 540) == 740)
        #expect(fit.width(toFit: CGSize(width: 574, height: 1120), in: baseArea, at: 540) == 580)
        #expect(fit.width(toFit: CGSize(width: 988, height: 1120), in: baseArea, at: 540) == 990)
        // 正好是整档的最小宽度：浮点误差不能让它多进一档
        #expect(fit.width(toFit: CGSize(width: 580, height: 1120), in: baseArea, at: 540) == 580)
        #expect(fit.width(toFit: CGSize(width: 850, height: 1120), in: baseArea, at: 540) == 850)
    }

    @Test("高度超了：按高度等比换档，宽高比不变（不会又高又窄）")
    func taller() {
        // 540 × 1300 / 1120 ≈ 627 → 630；630 那一档可放区域高 1369 - 25 - 33 = 1311，装得下 1300
        #expect(fit.width(toFit: CGSize(width: 540, height: 1300), in: baseArea, at: 540) == 630)
        // 宽高都超：按超得多的那一边（宽 600 > 高 540×1200/1120 ≈ 579）
        #expect(fit.width(toFit: CGSize(width: 600, height: 1200), in: baseArea, at: 540) == 600)
    }

    @Test("比最宽一档还宽：用最宽一档")
    func capped() {
        #expect(fit.width(toFit: CGSize(width: 1200, height: 1120), in: baseArea, at: 540) == 1080)
        #expect(fit.width(toFit: CGSize(width: 1200, height: 1838), in: CGSize(width: 1080, height: 1838), at: 1080) == 1080)
    }

    @Test("已经在更宽的一档时窗口又变宽了：按这一档的比例往上找")
    func fromWiderStep() {
        #expect(fit.width(toFit: CGSize(width: 800, height: 1242), in: CGSize(width: 740, height: 1242), at: 740) == 800)
    }

    @Test("Snug 自己的面板跟着档位放大：小横条在手机上一样高")
    func scale() {
        #expect(ScreenFit.scale(screenWidth: 540) == 1)
        #expect(ScreenFit.scale(screenWidth: 1080) == 2)
        #expect(ScreenFit.homeBarHeight(screenWidth: 540) == 28)
        #expect(ScreenFit.homeBarHeight(screenWidth: 1080) == 56)
    }

    @Test("手势翻译的范围：整块手机屏，阈值按这一档放大")
    func touchRegion() {
        let screen = CGRect(x: 1512, y: 0, width: 1080, height: 1900)
        #expect(TouchRegion(phoneScreen: screen) == TouchRegion(frame: screen, scale: 2))
    }
}
