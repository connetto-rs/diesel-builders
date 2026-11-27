//! Tests for columns the database fills from their SQL `DEFAULT`, which an
//! insert leaves out until the builder holds a value for them.

mod shared;
use diesel::{connection::SimpleConnection, prelude::*};
use diesel_builders::prelude::*;

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = rules)]
/// Model for the `rules` table.
pub struct Rule {
    /// Primary key.
    id: i32,
    /// A plain column.
    name: String,
    /// A non-nullable column with a SQL default.
    #[table_model(database_default)]
    in_use: bool,
    /// A nullable column whose SQL default is not `NULL`.
    #[table_model(database_default)]
    note: Option<String>,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = counters)]
#[table_model(surrogate_key)]
/// Model for a table whose every written column has a SQL default.
pub struct Counter {
    /// Primary key.
    id: i32,
    /// A counter starting at its SQL default.
    #[table_model(database_default)]
    hits: i32,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = procedures)]
#[table_model(surrogate_key)]
/// Root table whose `status` its extension sets.
pub struct Procedure {
    /// Primary key.
    id: i32,
    /// A status with a SQL default.
    #[table_model(database_default)]
    status: String,
}

#[derive(Debug, Queryable, Clone, Selectable, Identifiable, PartialEq, TableModel)]
#[diesel(table_name = weighings)]
#[table_model(ancestors(procedures))]
/// Extension table whose status sets the root status.
pub struct Weighing {
    /// Primary key.
    id: i32,
    /// The status, copied into the root.
    #[same_as(procedures::status)]
    #[table_model(database_default)]
    copied_status: String,
    /// A column only the database fills.
    #[table_model(database_default)]
    scale: String,
}

diesel::allow_tables_to_appear_in_same_query!(procedures, weighings);

/// Opens a connection holding the test tables.
fn connection_with_tables() -> SqliteConnection {
    let mut connection = shared::establish_connection().expect("connection opens");
    // Diesel has no DSL for DDL.
    connection
        .batch_execute(
            "CREATE TABLE rules (
                id INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                in_use BOOLEAN NOT NULL DEFAULT TRUE,
                note TEXT DEFAULT 'none'
            );
            CREATE TABLE counters (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                hits INTEGER NOT NULL DEFAULT 7
            );
            CREATE TABLE procedures (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                status TEXT NOT NULL DEFAULT 'planned'
            );
            CREATE TABLE weighings (
                id INTEGER PRIMARY KEY NOT NULL REFERENCES procedures (id),
                copied_status TEXT NOT NULL DEFAULT 'planned',
                scale TEXT NOT NULL DEFAULT 'kitchen'
            );",
        )
        .expect("tables are created");
    connection
}

#[test]
fn unset_columns_take_their_sql_default() {
    let mut connection = connection_with_tables();

    let builder = rules::table::builder().id(1).name("fridge");
    assert_eq!(builder.may_get_column_ref::<rules::in_use>(), None);
    assert_eq!(builder.may_get_column_ref::<rules::note>(), None);

    let rule = builder.insert(&mut connection).expect("builder inserts without the defaults");
    assert_eq!(rule.in_use(), &true);
    assert_eq!(rule.note(), &Some("none".to_owned()));

    let loaded: Rule = Rule::find(&1, &mut connection).expect("rule loads");
    assert_eq!(loaded, rule);
}

#[test]
fn set_columns_are_written() {
    let mut connection = connection_with_tables();

    let rule = rules::table::builder()
        .id(1)
        .name("freezer")
        .in_use(false)
        .note(None::<String>)
        .insert(&mut connection)
        .expect("builder writes the columns it holds");
    assert_eq!(rule.in_use(), &false);
    assert_eq!(rule.note(), &None);
}

#[test]
fn a_row_of_defaults_only_is_inserted() {
    let mut connection = connection_with_tables();

    let counter = counters::table::builder().insert(&mut connection).expect("an empty row inserts");
    assert_eq!(counter.hits(), &7);

    let counter = counters::table::builder()
        .hits(3)
        .insert(&mut connection)
        .expect("a set default column inserts");
    assert_eq!(counter.hits(), &3);
}

#[test]
fn values_reaching_a_column_through_same_as_are_written() {
    let mut connection = connection_with_tables();

    let weighing = weighings::table::builder()
        .copied_status("completed")
        .insert(&mut connection)
        .expect("the propagated status is written");
    assert_eq!(weighing.copied_status(), "completed");
    assert_eq!(weighing.scale(), "kitchen");
    let procedure: Procedure = weighing.ancestor(&mut connection).expect("root loads");
    assert_eq!(procedure.status(), "completed");

    let weighing = weighings::table::builder()
        .insert(&mut connection)
        .expect("ancestor and extension insert from their defaults");
    assert_eq!(weighing.copied_status(), "planned");
    let procedure: Procedure = weighing.ancestor(&mut connection).expect("root loads");
    assert_eq!(procedure.status(), "planned");
}
