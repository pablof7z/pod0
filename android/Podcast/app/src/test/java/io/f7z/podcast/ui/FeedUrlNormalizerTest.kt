package io.f7z.podcast.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class FeedUrlNormalizerTest {
    @Test
    fun addsHttpsSchemeForBareHosts() {
        assertEquals(
            "https://feeds.example.com/show.xml",
            FeedUrlNormalizer.normalizedFeedUrl("  feeds.example.com/show.xml  "),
        )
    }

    @Test
    fun preservesHttpAndHttpsFeeds() {
        assertEquals(
            "http://feeds.example.com/show.xml",
            FeedUrlNormalizer.normalizedFeedUrl("http://feeds.example.com/show.xml"),
        )
        assertEquals(
            "https://feeds.example.com/show.xml",
            FeedUrlNormalizer.normalizedFeedUrl("https://feeds.example.com/show.xml"),
        )
    }

    @Test
    fun rejectsNonHttpSchemesAndInvalidHosts() {
        assertNull(FeedUrlNormalizer.normalizedFeedUrl("ftp://example.com/feed.xml"))
        assertNull(FeedUrlNormalizer.normalizedFeedUrl("mailto:show@example.com"))
        assertNull(FeedUrlNormalizer.normalizedFeedUrl("https:///feed.xml"))
        assertNull(FeedUrlNormalizer.normalizedFeedUrl(""))
    }
}
