//! Tests for primary keys holding a column generated as a constant, which
//! builders never write and never offer a setter for.

mod shared;
use diesel::{connection::SimpleConnection, prelude::*};
use diesel_builders::prelude::*;

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = users, primary_key(namespace_id, id))]
/// A root keyed by a constant namespace and an identifier.
pub struct User {
    /// The namespace every user belongs to.
    #[table_model(generated = "acme")]
    namespace_id: String,
    /// The identifier within the namespace.
    id: i32,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = operators, primary_key(namespace_id, id))]
#[table_model(ancestors(users))]
/// A descendant sharing the namespace of its ancestor.
pub struct Operator {
    /// The namespace, equal to the ancestor's.
    #[table_model(generated = "acme")]
    namespace_id: String,
    /// The identifier, handed down from the ancestor.
    id: i32,
    /// The operator's title.
    title: String,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = auditors, primary_key(namespace_id, id))]
#[table_model(ancestors(users))]
/// A descendant whose namespace differs from its ancestor's.
pub struct Auditor {
    /// A namespace that no user row can hold.
    #[table_model(generated = "other")]
    namespace_id: String,
    /// The identifier, handed down from the ancestor.
    id: i32,
}

diesel::allow_tables_to_appear_in_same_query!(users, operators, auditors);

/// Opens a connection whose `users` and `operators` views abort any insert
/// naming `namespace_id`, as PostgreSQL refuses a value for a generated
/// column.
fn connection_with_views() -> SqliteConnection {
    let mut connection = shared::establish_connection().expect("connection opens");
    // Diesel has no DSL for DDL.
    connection
        .batch_execute(
            "CREATE TABLE user_rows (
                namespace_id TEXT NOT NULL DEFAULT 'acme',
                id INTEGER NOT NULL,
                PRIMARY KEY (namespace_id, id)
            );
            CREATE VIEW users AS SELECT namespace_id, id FROM user_rows;
            CREATE TRIGGER users_insert INSTEAD OF INSERT ON users BEGIN
                SELECT RAISE(ABORT, 'users.namespace_id written')
                WHERE NEW.namespace_id IS NOT NULL;
                INSERT INTO user_rows (id) VALUES (NEW.id);
            END;
            CREATE TABLE operator_rows (
                namespace_id TEXT NOT NULL DEFAULT 'acme',
                id INTEGER NOT NULL,
                title TEXT NOT NULL,
                PRIMARY KEY (namespace_id, id),
                FOREIGN KEY (namespace_id, id) REFERENCES user_rows (namespace_id, id)
            );
            CREATE VIEW operators AS SELECT namespace_id, id, title FROM operator_rows;
            CREATE TRIGGER operators_insert INSTEAD OF INSERT ON operators BEGIN
                SELECT RAISE(ABORT, 'operators.namespace_id written')
                WHERE NEW.namespace_id IS NOT NULL;
                INSERT INTO operator_rows (id, title) VALUES (NEW.id, NEW.title);
            END;
            CREATE TABLE auditors (
                namespace_id TEXT NOT NULL DEFAULT 'other',
                id INTEGER NOT NULL,
                PRIMARY KEY (namespace_id, id),
                FOREIGN KEY (namespace_id, id) REFERENCES user_rows (namespace_id, id)
            );",
        )
        .expect("views are created");
    connection
}

#[test]
fn builder_insert_leaves_constant_key_column_out() {
    let mut connection = connection_with_views();

    let user = users::table::builder().id(1).insert(&mut connection).expect("user inserts");

    assert_eq!(user, User { namespace_id: "acme".to_owned(), id: 1 });
    assert_eq!(User::find((&"acme".to_owned(), &1), &mut connection), Ok(user));
}

#[test]
fn descendant_receives_ancestor_key_with_equal_constant() {
    let mut connection = connection_with_views();

    let operator = SetUserId::id(operators::table::builder(), 1)
        .title("Root")
        .insert(&mut connection)
        .expect("operator inserts");
    assert_eq!(
        operator,
        Operator { namespace_id: "acme".to_owned(), id: 1, title: "Root".to_owned() }
    );
    let user: User = operator.ancestor(&mut connection).expect("ancestor loads");
    assert_eq!(user, User { namespace_id: "acme".to_owned(), id: 1 });

    let (user, (operator,)) = SetUserId::id(operators::table::builder(), 2)
        .title("Deputy")
        .insert_nested(&mut connection)
        .expect("nested operator inserts");
    assert_eq!(user, User { namespace_id: "acme".to_owned(), id: 2 });
    assert_eq!(
        operator,
        Operator { namespace_id: "acme".to_owned(), id: 2, title: "Deputy".to_owned() }
    );
}

#[test]
fn descendant_with_differing_constant_fails_before_the_database() {
    let mut connection = connection_with_views();

    let error = SetUserId::id(auditors::table::builder(), 1)
        .insert(&mut connection)
        .expect_err("the ancestor's namespace cannot be the auditor's");

    assert_eq!(
        error.to_string(),
        "Validation error: `auditors.namespace_id` is generated as a constant and cannot hold \
         another value"
    );
    let users_written: i64 =
        users::table.count().get_result(&mut connection).expect("users are counted");
    assert_eq!(users_written, 0);
}
