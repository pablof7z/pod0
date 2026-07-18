/// Native route vocabulary used while translating AVAudioSession events.
/// The generated Pod0Core route enum is the cross-platform wire contract.
enum NativePlaybackAudioRoute: String, Equatable, Sendable {
    case builtIn
    case wired
    case bluetooth
    case airPlay
    case car
    case external
    case unknown
}

enum NativePlaybackInterruption: String, Equatable, Sendable {
    case none
    case began
    case endedShouldResume
    case endedShouldRemainPaused
}
