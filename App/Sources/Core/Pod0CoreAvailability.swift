import Pod0Core

/// Compile-time proof that the native app consumes the one generated facade
/// module. Domain cutover remains feature-disabled until its migration issue.
enum Pod0CoreAvailability {
    static let facadeType = Pod0Facade.self
}
