//! Per-backend insertion of a row, returning the row as the database stored
//! it.

use crate::TableExt;
#[cfg(feature = "sqlite")]
use crate::{LoadQueryBuilder, TypedNestedTuple};

/// A Diesel backend on which a builder inserts a row into `T` and reads back
/// the model the database stored.
#[diagnostic::on_unimplemented(
    message = "diesel-builders cannot insert rows on the backend `{Self}`",
    note = "enable the `sqlite` or `postgres` feature of diesel-builders for the backend in use"
)]
pub(crate) trait InsertStoredRow<Conn, T: TableExt, Values> {
    /// Inserts `values` into `T` and returns the stored row.
    ///
    /// # Errors
    ///
    /// Returns the database error of the insert or of reading the row back.
    fn insert_stored_row(values: Values, conn: &mut Conn) -> diesel::QueryResult<T::Model>;
}

/// `RETURNING` reports the stored row, so one statement inserts and reads it.
#[cfg(feature = "postgres")]
impl<Conn, T, Values> InsertStoredRow<Conn, T, Values> for diesel::pg::Pg
where
    T: TableExt,
    Values: diesel::Insertable<T>,
    diesel::query_builder::InsertStatement<T, Values::Values>:
        for<'query> diesel::query_dsl::LoadQuery<'query, Conn, T::Model>,
{
    fn insert_stored_row(values: Values, conn: &mut Conn) -> diesel::QueryResult<T::Model> {
        use diesel::RunQueryDsl;
        diesel::insert_into(T::default()).values(values).get_result(conn)
    }
}

/// The primary key values of a row of `T`.
#[cfg(feature = "sqlite")]
type PrimaryKeyValues<T> =
    <<T as TableExt>::NestedPrimaryKeyColumns as TypedNestedTuple>::NestedTupleValueType;

/// `RETURNING` on a view that `INSTEAD OF` triggers write through reports the
/// row as the statement wrote it, with `NULL` for every column the database
/// fills in. The insert therefore returns only the primary key, and the row
/// is loaded by that key.
#[cfg(feature = "sqlite")]
impl<Conn, T, Values> InsertStoredRow<Conn, T, Values> for diesel::sqlite::Sqlite
where
    T: TableExt<NestedPrimaryKeyColumns: LoadQueryBuilder>,
    Values: diesel::Insertable<T>,
    PrimaryKeyValues<T>: tuplities::prelude::NestedTupleInto<PrimaryKeyValues<T>>,
    diesel::dsl::Returning<
        diesel::query_builder::InsertStatement<T, Values::Values>,
        T::NestedPrimaryKeyColumns,
    >: diesel::query_builder::Query
        + for<'query> diesel::query_dsl::LoadQuery<'query, Conn, PrimaryKeyValues<T>>,
    <T::NestedPrimaryKeyColumns as LoadQueryBuilder>::LoadQuery:
        for<'query> diesel::query_dsl::LoadQuery<'query, Conn, T::Model>,
{
    fn insert_stored_row(values: Values, conn: &mut Conn) -> diesel::QueryResult<T::Model> {
        use diesel::RunQueryDsl;
        let key: PrimaryKeyValues<T> = diesel::insert_into(T::default())
            .values(values)
            .returning(T::NestedPrimaryKeyColumns::default())
            .get_result(conn)?;
        T::NestedPrimaryKeyColumns::load_query(key).get_result(conn)
    }
}
