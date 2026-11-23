//! Tests for builder inserts into `SQLite` views that `INSTEAD OF` triggers
//! write through, whose `RETURNING` rows hold `NULL` for every column the
//! database fills in.

mod shared;
use diesel::{connection::SimpleConnection, prelude::*};
use diesel_builders::prelude::*;

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = users)]
/// Model for the `users` view over `users_rows`.
pub struct User {
    /// Primary key.
    id: i32,
    /// A column the backing table generates.
    #[table_model(generated)]
    kind: String,
    /// A column the insert trigger writes.
    #[table_model(generated)]
    state: String,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = memberships, primary_key(organization_id, user_id))]
/// Model for the `memberships` view over `membership_rows`.
pub struct Membership {
    /// First primary key column.
    organization_id: i32,
    /// Second primary key column.
    user_id: i32,
    /// A column the backing table defaults.
    #[table_model(generated)]
    role: String,
}

/// Opens a connection holding the `users` and `memberships` views.
fn connection_with_views() -> SqliteConnection {
    let mut connection = shared::establish_connection().expect("connection opens");
    // Diesel has no DSL for DDL.
    connection
        .batch_execute(
            "CREATE TABLE users_rows (
                id INTEGER PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL GENERATED ALWAYS AS ('user') STORED,
                state TEXT NOT NULL
            );
            CREATE VIEW users AS SELECT id, kind, state FROM users_rows;
            CREATE TRIGGER users_insert INSTEAD OF INSERT ON users BEGIN
                INSERT INTO users_rows (id, state) VALUES (NEW.id, 'active');
            END;
            CREATE TABLE membership_rows (
                organization_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                role TEXT NOT NULL DEFAULT 'member',
                PRIMARY KEY (organization_id, user_id)
            );
            CREATE VIEW memberships AS
                SELECT organization_id, user_id, role FROM membership_rows;
            CREATE TRIGGER memberships_insert INSTEAD OF INSERT ON memberships BEGIN
                INSERT INTO membership_rows (organization_id, user_id)
                VALUES (NEW.organization_id, NEW.user_id);
            END;",
        )
        .expect("views are created");
    connection
}

#[test]
fn builder_insert_into_view_returns_stored_row() {
    let mut connection = connection_with_views();

    let user = users::table::builder().id(1).insert(&mut connection).expect("user inserts");

    assert_eq!(user, User { id: 1, kind: "user".to_owned(), state: "active".to_owned() });
}

#[test]
fn builder_insert_into_view_with_composite_key_returns_stored_row() {
    let mut connection = connection_with_views();
    memberships::table::builder()
        .organization_id(1)
        .user_id(1)
        .insert(&mut connection)
        .expect("first membership inserts");

    let membership = memberships::table::builder()
        .organization_id(1)
        .user_id(2)
        .insert(&mut connection)
        .expect("second membership inserts");

    assert_eq!(
        membership,
        Membership { organization_id: 1, user_id: 2, role: "member".to_owned() }
    );
}
