package io.f7z.podcast.ui

import java.net.URI

object FeedUrlNormalizer {
    private val schemePattern = Regex("^[A-Za-z][A-Za-z0-9+.-]*:")

    fun normalizedFeedUrl(input: String?): String? {
        val trimmed = input?.trim().orEmpty()
        if (trimmed.isEmpty()) return null

        val candidate = if (schemePattern.containsMatchIn(trimmed)) {
            trimmed
        } else {
            "https://$trimmed"
        }

        val uri = try {
            URI(candidate)
        } catch (_: IllegalArgumentException) {
            return null
        }
        val scheme = uri.scheme?.lowercase() ?: return null
        if (scheme != "http" && scheme != "https") return null
        if (uri.host.isNullOrBlank()) return null
        return uri.toString()
    }
}
