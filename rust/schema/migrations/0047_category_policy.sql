CREATE TABLE pod0_category_settings(
    category_id BLOB PRIMARY KEY NOT NULL
        REFERENCES pod0_categories(category_id) ON DELETE CASCADE
        CHECK(length(category_id)=16),
    auto_download_code INTEGER
        CHECK(auto_download_code IS NULL OR auto_download_code IN (1,2,3)),
    auto_download_latest_count INTEGER
        CHECK(auto_download_latest_count IS NULL
            OR auto_download_latest_count BETWEEN 0 AND 65535),
    wifi_only INTEGER CHECK(wifi_only IS NULL OR wifi_only IN (0,1)),
    rag_enabled INTEGER NOT NULL CHECK(rag_enabled IN (0,1)),
    notifications_enabled INTEGER NOT NULL CHECK(notifications_enabled IN (0,1)),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=0),
    CHECK((auto_download_code IS NULL)=(wifi_only IS NULL)),
    CHECK(
        (auto_download_code=2 AND auto_download_latest_count IS NOT NULL)
        OR (auto_download_code IS NULL AND auto_download_latest_count IS NULL)
        OR (auto_download_code IN (1,3) AND auto_download_latest_count IS NULL)
    )
) STRICT;

-- Categories created by the dormant Rust store predate settings ownership.
-- Backfill their permissive native defaults without inventing an override.
INSERT INTO pod0_category_settings(
    category_id,auto_download_code,auto_download_latest_count,wifi_only,
    rag_enabled,notifications_enabled,updated_at_ms
)
SELECT category_id,NULL,NULL,NULL,1,1,updated_at_ms FROM pod0_categories;
