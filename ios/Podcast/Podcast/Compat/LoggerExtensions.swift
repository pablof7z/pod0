// Compat shim — replaced when proper logging infrastructure lands.
//
// Legacy views call `Logger.app("Category")` to obtain a category-tagged
// `os.Logger`. This stub adapts that call to the standard subsystem so the
// views compile without modification.

import Foundation
import os

extension Logger {
    /// Category-tagged logger using the app bundle as subsystem. Drop-in
    /// replacement for the legacy `Logger.app("...")` helper.
    static func app(_ category: String) -> Logger {
        Logger(subsystem: Bundle.main.bundleIdentifier ?? "io.f7z.podcast", category: category)
    }
}
