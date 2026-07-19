ALTER TABLE pod0_podcasts
ADD COLUMN library_visible INTEGER NOT NULL DEFAULT 1
    CHECK(library_visible IN (0, 1));

CREATE INDEX pod0_podcasts_library_visible_idx
ON pod0_podcasts(library_visible);
