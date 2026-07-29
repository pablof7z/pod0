CREATE TABLE pod0_category_state(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    collection_revision INTEGER NOT NULL CHECK(collection_revision>=0)
) STRICT;

INSERT INTO pod0_category_state(singleton,collection_revision) VALUES(1,0);

CREATE TABLE pod0_categories(
    category_id BLOB PRIMARY KEY NOT NULL CHECK(length(category_id)=16),
    category_revision INTEGER NOT NULL CHECK(category_revision>=1),
    name TEXT NOT NULL
        CHECK(length(CAST(name AS BLOB)) BETWEEN 1 AND 128),
    slug TEXT NOT NULL CHECK(length(CAST(slug AS BLOB))<=128),
    description TEXT NOT NULL
        CHECK(length(CAST(description AS BLOB)) BETWEEN 1 AND 1024),
    color_hex TEXT
        CHECK(color_hex IS NULL OR length(color_hex) IN (7,9)),
    origin_code INTEGER NOT NULL CHECK(origin_code IN (1,2,3)),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    deleted INTEGER NOT NULL CHECK(deleted IN (0,1)),
    created_command_id BLOB
        CHECK(created_command_id IS NULL OR length(created_command_id)=16)
) STRICT;

CREATE INDEX pod0_categories_active_name_v1
    ON pod0_categories(deleted,name,category_id);

-- Membership is a join rather than a JSON column so an item can belong to
-- several categories, and so removing one membership never rewrites a row
-- that another concurrent command is also editing.
CREATE TABLE pod0_category_members(
    category_id BLOB NOT NULL
        REFERENCES pod0_categories(category_id) ON DELETE CASCADE
        CHECK(length(category_id)=16),
    item_id BLOB NOT NULL CHECK(length(item_id)=16),
    item_kind_code INTEGER NOT NULL CHECK(item_kind_code IN (1,2)),
    added_at_ms INTEGER NOT NULL CHECK(added_at_ms>=0),
    PRIMARY KEY(category_id,item_id)
) STRICT;

CREATE INDEX pod0_category_members_item_v1
    ON pod0_category_members(item_id,category_id);

CREATE INDEX pod0_category_members_recent_v1
    ON pod0_category_members(category_id,added_at_ms DESC,item_id);
