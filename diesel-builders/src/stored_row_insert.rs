//! Writing the rows of an insert and reading back the rows the database
//! stored.
//!
//! A nested insert writes every row before it reads any. Row-level security
//! policies can make a row visible only once the rows of other tables exist,
//! so a row read right after its own insert may not be visible yet.

use diesel::{RunQueryDsl, query_dsl::LoadQuery};
use tuplities::prelude::{IntoNestedTupleOption, NestedTupleInto, NestedTupleOption};

use crate::{
    LoadQueryBuilder, MayGetNestedColumns, TableExt, TypedNestedTuple,
    builder_bundle::BundlableTableExt,
    insert_row::{InsertRow, NewRecordColumns, Returning},
};

/// The key source of a table whose primary key the builder holds before the
/// insert.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SuppliedKey;

/// The key source of a table whose primary key the database assigns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SurrogateKey;

/// The primary key values of a row of `T`.
pub(crate) type PrimaryKeyValues<T> =
    <<T as TableExt>::NestedPrimaryKeyColumns as TypedNestedTuple>::NestedTupleColumnType;

/// A table whose new values convert into the row an insert writes.
pub(crate) trait InsertValues: BundlableTableExt {
    /// The row an insert writes.
    type Row;

    /// Converts new values that hold every column the database does not
    /// supply into the insert of their row.
    fn insert_row(new_values: Self::NewValues) -> InsertRow<Self, Self::Row>;
}

impl<T> InsertValues for T
where
    T: BundlableTableExt<NewRecord: NewRecordColumns<Values = T::NewValues>>,
{
    type Row = <T::NewRecord as NewRecordColumns>::Row;

    fn insert_row(new_values: Self::NewValues) -> InsertRow<Self, Self::Row> {
        InsertRow::new(T::NewRecord::row(new_values))
    }
}

/// A key source writing a row of `T` without reading it back, returning the
/// row's primary key.
pub(crate) trait WriteRow<Conn, T: InsertValues> {
    /// Writes the row holding `new_values` and returns its primary key.
    ///
    /// # Errors
    ///
    /// Returns the database error of the insert.
    fn write_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<PrimaryKeyValues<T>>;
}

/// The key is taken from the new values, so the insert reports nothing back
/// and applies no `SELECT` policy to the new row.
impl<Conn, T> WriteRow<Conn, T> for SuppliedKey
where
    T: InsertValues,
    T::NewValues: MayGetNestedColumns<T::NestedPrimaryKeyColumns>,
    <PrimaryKeyValues<T> as IntoNestedTupleOption>::IntoOptions:
        NestedTupleOption<Transposed = PrimaryKeyValues<T>>,
    InsertRow<T, T::Row>: diesel::query_dsl::methods::ExecuteDsl<Conn>,
    Conn: diesel::Connection,
{
    fn write_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<PrimaryKeyValues<T>> {
        #[expect(
            clippy::expect_used,
            reason = "the primary key columns of a table without a surrogate key belong to its \
                      new record and cannot be database defaults, and callers check that such \
                      columns are present before writing"
        )]
        let key = <T::NewValues as MayGetNestedColumns<T::NestedPrimaryKeyColumns>>::may_get_nested_columns(
            &new_values,
        )
        .transpose()
        .expect("every primary key column was already proven present");
        T::insert_row(new_values).execute(conn)?;
        Ok(key)
    }
}

/// The database assigns the key, so the insert returns it.
impl<Conn, T> WriteRow<Conn, T> for SurrogateKey
where
    T: InsertValues,
    InsertRow<T, T::Row, Returning<T::NestedPrimaryKeyColumns>>:
        for<'query> LoadQuery<'query, Conn, PrimaryKeyValues<T>>,
{
    fn write_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<PrimaryKeyValues<T>> {
        T::insert_row(new_values).returning(T::NestedPrimaryKeyColumns::default()).get_result(conn)
    }
}

/// The primary key values of a row of `T` as a filter takes them.
type PrimaryKeyFilter<T> =
    <<T as TableExt>::NestedPrimaryKeyColumns as TypedNestedTuple>::NestedTupleValueType;

/// A table whose stored rows load by primary key.
pub(crate) trait LoadStoredRow<Conn>: TableExt {
    /// Loads the stored row with the given primary key.
    ///
    /// # Errors
    ///
    /// Returns the database error of the query, or
    /// [`NotFound`](diesel::result::Error::NotFound) when the row is not
    /// visible.
    fn load_stored_row(
        key: PrimaryKeyValues<Self>,
        conn: &mut Conn,
    ) -> diesel::QueryResult<Self::Model>;
}

impl<Conn, T> LoadStoredRow<Conn> for T
where
    T: TableExt<NestedPrimaryKeyColumns: LoadQueryBuilder>,
    PrimaryKeyValues<T>: NestedTupleInto<PrimaryKeyFilter<T>>,
    <T::NestedPrimaryKeyColumns as LoadQueryBuilder>::LoadQuery:
        for<'query> LoadQuery<'query, Conn, T::Model>,
{
    fn load_stored_row(key: PrimaryKeyValues<T>, conn: &mut Conn) -> diesel::QueryResult<T::Model> {
        T::NestedPrimaryKeyColumns::load_query(key).get_result(conn)
    }
}

/// A Diesel backend on which a builder writes the last row of an insert into
/// `T` and reads back the model the database stored.
#[diagnostic::on_unimplemented(
    message = "diesel-builders cannot insert rows on the backend `{Self}`",
    note = "enable the `sqlite` or `postgres` feature of diesel-builders for the backend in use"
)]
pub(crate) trait InsertStoredRow<Conn, T: InsertValues> {
    /// Writes the row holding `new_values` and returns the stored row.
    ///
    /// # Errors
    ///
    /// Returns the database error of the insert or of reading the row back.
    fn insert_stored_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<T::Model>;
}

/// The last row is written once every other row exists, so `RETURNING`
/// reports it in the same statement.
#[cfg(feature = "postgres")]
impl<Conn, T> InsertStoredRow<Conn, T> for diesel::pg::Pg
where
    T: InsertValues,
    InsertRow<T, T::Row, Returning<T::AllColumns>>: for<'query> LoadQuery<'query, Conn, T::Model>,
{
    fn insert_stored_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<T::Model> {
        T::insert_row(new_values).returning(<T as diesel::Table>::all_columns()).get_result(conn)
    }
}

/// `RETURNING` on a view that `INSTEAD OF` triggers write through reports the
/// row as the statement wrote it, with `NULL` for every column the database
/// fills in, so the row is written and then loaded by its key.
#[cfg(feature = "sqlite")]
impl<Conn, T> InsertStoredRow<Conn, T> for diesel::sqlite::Sqlite
where
    T: InsertValues + LoadStoredRow<Conn>,
    T::KeySource: WriteRow<Conn, T>,
{
    fn insert_stored_row(
        new_values: T::NewValues,
        conn: &mut Conn,
    ) -> diesel::QueryResult<T::Model> {
        let key = <T::KeySource as WriteRow<Conn, T>>::write_row(new_values, conn)?;
        T::load_stored_row(key, conn)
    }
}
