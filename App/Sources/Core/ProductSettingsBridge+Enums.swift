import Pod0Core

extension ProductSettingsBridge {
    static func headphoneAction(_ value: HeadphoneGestureAction) -> HeadphoneGestureSetting {
        switch value {
        case .skipForward: .skipForward
        case .skipBackward: .skipBackward
        case .nextChapter: .nextChapter
        case .previousChapter: .previousChapter
        case .clipNow: .clipNow
        case .none: .none
        }
    }

    static func headphoneAction(_ value: HeadphoneGestureSetting) -> HeadphoneGestureAction {
        switch value {
        case .skipForward: .skipForward
        case .skipBackward: .skipBackward
        case .nextChapter: .nextChapter
        case .previousChapter: .previousChapter
        case .clipNow: .clipNow
        case .none: .none
        }
    }

    static func transcriptionProvider(_ value: STTProvider) -> SpeechTranscriptionSetting {
        switch value {
        case .elevenLabsScribe: .elevenLabsScribe
        case .assemblyAI: .assemblyAi
        case .openRouterWhisper: .openRouterWhisper
        case .appleNative: .appleNative
        }
    }

    static func transcriptionProvider(_ value: SpeechTranscriptionSetting) -> STTProvider {
        switch value {
        case .elevenLabsScribe: .elevenLabsScribe
        case .assemblyAi: .assemblyAI
        case .openRouterWhisper: .openRouterWhisper
        case .appleNative: .appleNative
        }
    }
}
