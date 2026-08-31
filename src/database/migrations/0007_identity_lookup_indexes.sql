CREATE INDEX drive_items_owner_email_normalized_index
    ON drive_items (lower(owner_email));

CREATE INDEX drive_permissions_email_normalized_index
    ON drive_permissions (lower(email_address));
