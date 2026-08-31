ALTER TABLE drive_items ADD COLUMN cumulative_size_bytes INTEGER;

CREATE INDEX drive_items_parent_path_index
    ON drive_items (remote_name, parent_path, is_deleted);
