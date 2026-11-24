import Foundation
import Testing
@testable import SnugCore

@Suite("配置 AppConfig")
struct ConfigModelTests {
    @Test("默认值：手机屏开着，手势关着（向日葵下强制关闭），截图键关着，字号默认放大 2 级，还没看过欢迎页，从 GitHub 更新")
    func defaults() {
        let c = AppConfig()
        #expect(c.phoneScreen)
        #expect(!c.onlyVirtual)
        #expect(!c.gestures)
        #expect(!c.screenshot)
        #expect(c.zoomSteps == 2)
        #expect(!c.welcomed)
        #expect(c.updateSource == .github)
        #expect(c.lastVersion == nil)
        #expect(c.phone == .standard)
    }

    @Test("手机型号：老配置里没有就用默认（iPhone 17 Pro Max）；手改坏了（横着写、不是正数、缺字段）用默认，不连累别的设置")
    func phone() throws {
        let decode = { (json: String) in try JSONDecoder().decode(AppConfig.self, from: Data(json.utf8)) }
        #expect(try decode(#"{"zoomSteps":3}"#).phone == .standard)
        let custom = try decode(#"{"phone":{"screen":{"width":1080,"height":2400}}}"#)
        #expect(custom.phone == PhoneChoice(model: nil, screen: PhoneScreen(width: 1080, height: 2400)))
        for bad in [#"{"width":2400,"height":1080}"#, #"{"width":0,"height":2400}"#, #"{"width":1080}"#] {
            let c = try decode(#"{"zoomSteps":3,"phone":{"screen":\#(bad)}}"#)
            #expect(c.phone == .standard)
            #expect(c.zoomSteps == 3)
        }
    }

    @Test("不认识的更新源（比如更新的版本写的）用默认值，不把整份配置判为损坏")
    func unknownUpdateSource() throws {
        let c = try JSONDecoder().decode(AppConfig.self, from: Data(#"{"updateSource":"mirror","zoomSteps":3}"#.utf8))
        #expect(c.updateSource == .github)
        #expect(c.zoomSteps == 3)
    }

    @Test("缺字段的 config.json 用默认值补齐；旧版留下的字段忽略，而不是整份判为损坏")
    func decodeMissingAndLegacyKeys() throws {
        let json = #"{"gestures":false,"devices":[{"name":"iPad"}],"keepOnlyVirtual":true}"#
        let c = try JSONDecoder().decode(AppConfig.self, from: Data(json.utf8))
        #expect(c.gestures == false)
        #expect(c.phoneScreen)
        #expect(!c.screenshot)
        #expect(c.zoomSteps == 2)
    }

    @Test("存量配置里手势开着，解码后也是关的（向日葵下强制关闭，不读存量值）")
    func gesturesForcedOff() throws {
        let c = try JSONDecoder().decode(AppConfig.self, from: Data(#"{"gestures":true}"#.utf8))
        #expect(!c.gestures)
    }

    @Test("手改出界的字号级数夹回 0...4")
    func clampsZoomSteps() throws {
        let high = try JSONDecoder().decode(AppConfig.self, from: Data(#"{"zoomSteps":9}"#.utf8))
        let low = try JSONDecoder().decode(AppConfig.self, from: Data(#"{"zoomSteps":-3}"#.utf8))
        #expect(high.zoomSteps == 4)
        #expect(low.zoomSteps == 0)
    }

    @Test("编码再解码保持不变")
    func roundTrip() throws {
        var c = AppConfig()
        c.phoneScreen = false
        c.onlyVirtual = true
        c.screenshot = true
        c.zoomSteps = 3
        c.welcomed = true
        c.updateSource = .github
        c.lastVersion = Version(major: 2, minor: 1, patch: 0)
        c.phone = PhoneChoice(model: "华为 Mate 70 Pro", screen: PhoneScreen(width: 1260, height: 2832))
        #expect(try JSONDecoder().decode(AppConfig.self, from: JSONEncoder().encode(c)) == c)
    }
}
