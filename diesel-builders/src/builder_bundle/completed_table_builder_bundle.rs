//! Submodule for the completed table builder bundle and related impls.

use diesel::{Column, associations::HasTable};
use tuplities::prelude::*;

use crate::{
    BuildableTable, BuilderError, BuilderResult, ColumnTyped, DiscretionarySameAsIndex,
    EitherValidationError, HasPrimaryKeyColumn, HasTableExt, IncompleteBuilderError,
    MandatorySameAsIndex, NestedColumns, TableBuilder, TableBuilderBundle, TableExt,
    TryMaySetValues, TrySetColumn, TrySetDiscretionarySameAsColumn, TrySetMandatorySameAsColumn,
    TrySetValues, TypedColumn, ValidateColumn, ValidateRecord,
    builder_bundle::{BundlableTableExt, impl_bundle_shared},
    horizontal_same_as_group::HorizontalSameAsGroupExt,
    insert_row::NewRecordColumns,
    insert_transaction::{InsertBuilder, InsertBundle},
    stored_row_insert::{InsertStoredRow, InsertValues, PrimaryKeyValues, WriteRow},
};

#[derive(Debug)]
/// The build-ready variant of a table builder bundle.
pub struct CompletedTableBuilderBundle<T: BundlableTableExt> {
    /// The insertable model for the table.
    insertable_model: T::NewValues,
    /// The mandatory associated builders relative to triangular same-as.
    nested_mandatory_associated_builders: T::MandatoryNestedBuilders,
    /// The discretionary associated builders relative to triangular same-as.
    nested_discretionary_associated_builders: T::OptionalDiscretionaryNestedBuilders,
}

impl_bundle_shared!(CompletedTableBuilderBundle);

impl<Key, C> crate::mutation_same_as::PrepareMandatoryColumn<Key, C>
    for CompletedTableBuilderBundle<<Key as Column>::Table>
where
    Key: MandatorySameAsIndex<Table: BundlableTableExt, ReferencedTable: BuildableTable>,
    C: TypedColumn<Table = Key::ReferencedTable>,
    <Key::Table as BundlableTableExt>::MandatoryNestedBuilders:
        NestedTupleIndexMut<Key::Idx, Element = TableBuilder<C::Table>>,
    TableBuilder<C::Table>: crate::mutation::PrepareColumn<C>,
{
    type Error = <TableBuilder<C::Table> as ValidateColumn<C>>::Error;
    type Prepared = <TableBuilder<C::Table> as crate::mutation::PrepareColumn<C>>::Prepared;

    fn has_mandatory_target(&self, context: &crate::mutation::MutationContext) -> bool {
        !context.excludes_mandatory(
            std::ptr::from_ref(self).cast::<()>(),
            <Key::Idx as typenum::Unsigned>::USIZE,
        )
    }

    fn prepare_mandatory_column(
        &self,
        value: C::ColumnType,
        context: &crate::mutation::MutationContext,
    ) -> Result<Self::Prepared, (C::ColumnType, Self::Error)> {
        <TableBuilder<C::Table> as crate::mutation::PrepareColumn<C>>::prepare_column(
            self.nested_mandatory_associated_builders.nested_index(),
            value,
            context,
        )
    }

    fn apply_mandatory_column(&mut self, prepared: Self::Prepared) {
        <TableBuilder<C::Table> as crate::mutation::PrepareColumn<C>>::apply_column(
            self.nested_mandatory_associated_builders.nested_index_mut(),
            prepared,
        );
    }
}

impl<Key: MandatorySameAsIndex<Table: BundlableTableExt, ReferencedTable: BuildableTable>, C>
    TrySetMandatorySameAsColumn<Key, C> for CompletedTableBuilderBundle<<Key as Column>::Table>
where
    C: TypedColumn<Table = Key::ReferencedTable>,
    <Key::Table as BundlableTableExt>::MandatoryNestedBuilders:
        NestedTupleIndexMut<Key::Idx, Element = TableBuilder<C::Table>>,
    TableBuilder<C::Table>: TrySetColumn<C>,
{
    type Error = <TableBuilder<C::Table> as ValidateColumn<C>>::Error;

    #[inline]
    fn try_set_mandatory_same_as_column(
        &mut self,
        value: impl Into<C::ColumnType>,
    ) -> Result<&mut Self, Self::Error> {
        self.nested_mandatory_associated_builders.nested_index_mut().try_set_column(value)?;
        Ok(self)
    }
}

impl<T> TryFrom<TableBuilderBundle<T>> for CompletedTableBuilderBundle<T>
where
    T: BundlableTableExt,
{
    type Error = IncompleteBuilderError;

    fn try_from(
        value: TableBuilderBundle<T>,
    ) -> Result<CompletedTableBuilderBundle<T>, Self::Error> {
        Ok(CompletedTableBuilderBundle {
            insertable_model: value.insertable_model,
            nested_mandatory_associated_builders: value
                .nested_mandatory_associated_builders
                .transpose_or(T::NestedMandatoryTriangularColumns::NESTED_COLUMN_NAMES)
                .map_err(|column_name| {
                    IncompleteBuilderError::MissingMandatoryTriangularField {
                        table_name: T::TABLE_NAME,
                        field_name: column_name,
                    }
                })?,
            nested_discretionary_associated_builders: value
                .nested_discretionary_associated_builders,
        })
    }
}

/// Trait defining the insertion of a builder into the database.
pub trait RecursiveBundleInsert<Error, Conn>: HasTableExt {
    /// Insert the builder's data into the database using the provided
    /// connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the insertion fails or if any database constraints
    /// are violated.
    ///
    /// # Examples
    ///
    /// Complete a checked bundle and insert the record it stages.
    ///
    /// ```rust
    /// # include!("../doctest_setup.rs");
    /// # use schema::*;
    /// # use diesel_builders::{CompletedTableBuilderBundle, RecursiveBundleInsert, TableBuilderBundle};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut conn = connection()?;
    /// let bundle = TableBuilderBundle::<users::table>::try_from_values(user_values("Ada", 20, Some("Ace"))).expect("valid values must be accepted");
    /// let completed: CompletedTableBuilderBundle<users::table> = bundle.try_into()?;
    /// let user = completed.recursive_bundle_insert(&mut conn)?;
    /// assert_eq!(user.get_column::<users::name>(), "Ada");
    /// # Ok(())
    /// # }
    /// ```
    fn recursive_bundle_insert(
        self,
        conn: &mut Conn,
    ) -> BuilderResult<<<Self as HasTable>::Table as TableExt>::Model, Error>;
}

/// Writing the associated rows of a bundle and completing its record.
pub(crate) trait PrepareInsert<Conn>: HasTableExt {
    /// The validation errors that this stage can produce.
    type ValidationError;

    /// Writes the associated rows, sets their keys, and validates the
    /// completed record, returning the new values of this row.
    ///
    /// # Errors
    ///
    /// Returns the insertion, incomplete, or validation error from any
    /// associated record or from this record.
    fn prepare_insert(
        self,
        conn: &mut Conn,
    ) -> BuilderResult<<Self::Table as TableExt>::NewValues, Self::ValidationError>;
}

/// The validation errors that inserting a bundle of `T` can produce.
type BundleValidationError<T, MandatoryError, DiscretionaryError> = EitherValidationError<
    MandatoryError,
    EitherValidationError<
        <<T as TableExt>::NewValues as TrySetValues<
            <T as BundlableTableExt>::NestedMandatoryTriangularColumns,
        >>::Error,
        EitherValidationError<
            DiscretionaryError,
            EitherValidationError<
                <<T as TableExt>::NewValues as TryMaySetValues<
                    <T as BundlableTableExt>::NestedDiscretionaryTriangularColumns,
                >>::Error,
                <T as TableExt>::RecordError,
            >,
        >,
    >,
>;

impl<T, Conn, MandatoryError, DiscretionaryError> PrepareInsert<Conn>
    for CompletedTableBuilderBundle<T>
where
    Conn: diesel::connection::LoadConnection,
    T: BundlableTableExt,
    T::NewValues: TrySetValues<T::NestedMandatoryTriangularColumns>
        + TryMaySetValues<T::NestedDiscretionaryTriangularColumns>
        + ValidateRecord<T>,
    T::NewRecord: NewRecordColumns<Values = T::NewValues>,
    T::MandatoryNestedBuilders: InsertTuple<
            Conn,
            Keys = T::NestedMandatoryPrimaryKeyTypes,
            ValidationError = MandatoryError,
        >,
    T::OptionalDiscretionaryNestedBuilders: InsertOptionTuple<
            Conn,
            Keys = T::OptionalNestedDiscretionaryPrimaryKeyTypes,
            ValidationError = DiscretionaryError,
        >,
{
    type ValidationError = BundleValidationError<T, MandatoryError, DiscretionaryError>;

    fn prepare_insert(
        mut self,
        conn: &mut Conn,
    ) -> BuilderResult<T::NewValues, Self::ValidationError> {
        let mandatory_primary_keys =
            self.nested_mandatory_associated_builders
                .insert_tuple(conn)
                .map_err(|error| error.map_validation(EitherValidationError::Left))?;
        self.insertable_model.try_set_values(mandatory_primary_keys).map_err(|(_, error)| {
            BuilderError::Validation(EitherValidationError::Right(EitherValidationError::Left(
                error,
            )))
        })?;
        let discretionary_primary_keys = self
            .nested_discretionary_associated_builders
            .insert_option_tuple(conn)
            .map_err(|error| {
                error.map_validation(|error| {
                    EitherValidationError::Right(EitherValidationError::Right(
                        EitherValidationError::Left(error),
                    ))
                })
            })?;
        self.insertable_model.try_may_set_values(discretionary_primary_keys).map_err(
            |(_, error)| {
                BuilderError::Validation(EitherValidationError::Right(
                    EitherValidationError::Right(EitherValidationError::Right(
                        EitherValidationError::Left(error),
                    )),
                ))
            },
        )?;

        // Check the required fields without moving the stored values, so that
        // the record validator observes the complete final values.
        if let Some(column_name) = T::NewRecord::first_missing(&self.insertable_model) {
            return Err(BuilderError::Incomplete(IncompleteBuilderError::MissingMandatoryField {
                table_name: T::TABLE_NAME,
                field_name: column_name,
            }));
        }

        self.insertable_model.validate_record().map_err(|error| {
            BuilderError::Validation(EitherValidationError::Right(EitherValidationError::Right(
                EitherValidationError::Right(EitherValidationError::Right(error)),
            )))
        })?;

        Ok(self.insertable_model)
    }
}

impl<T, Conn> InsertBundle<Conn> for CompletedTableBuilderBundle<T>
where
    Conn: diesel::Connection,
    T: InsertValues,
    Self: PrepareInsert<Conn, Table = T>,
    T::KeySource: WriteRow<Conn, T>,
    Conn::Backend: InsertStoredRow<Conn, T>,
{
    type ValidationError = <Self as PrepareInsert<Conn>>::ValidationError;

    fn insert_bundle(
        self,
        conn: &mut Conn,
    ) -> BuilderResult<<T as TableExt>::Model, Self::ValidationError> {
        let new_values = self.prepare_insert(conn)?;
        Ok(<Conn::Backend as InsertStoredRow<Conn, T>>::insert_stored_row(new_values, conn)?)
    }

    fn insert_bundle_key(
        self,
        conn: &mut Conn,
    ) -> BuilderResult<PrimaryKeyValues<T>, Self::ValidationError> {
        let new_values = self.prepare_insert(conn)?;
        Ok(<T::KeySource as WriteRow<Conn, T>>::write_row(new_values, conn)?)
    }
}

/// The value of the primary key column of `T`.
type PrimaryKeyValue<T> = <<T as diesel::Table>::PrimaryKey as ColumnTyped>::ColumnType;

/// Trait defining the insertion of a tuple of builders into the database.
trait InsertTuple<Conn> {
    /// The primary keys of the rows the tuple's builders write.
    type Keys;
    /// The validation errors of the tuple's builders.
    type ValidationError;

    /// Writes the rows of the tuple's builders and returns their primary
    /// keys.
    ///
    /// # Errors
    ///
    /// Returns an error if any insertion fails, if any remaining validation
    /// fails, or if any database constraints are violated.
    fn insert_tuple(self, conn: &mut Conn) -> BuilderResult<Self::Keys, Self::ValidationError>;
}

impl<Conn> InsertTuple<Conn> for () {
    type Keys = ();
    type ValidationError = std::convert::Infallible;

    #[inline]
    fn insert_tuple(self, _conn: &mut Conn) -> BuilderResult<(), Self::ValidationError> {
        Ok(())
    }
}

impl<Conn, T> InsertTuple<Conn> for (T,)
where
    T: InsertBuilder<Conn, Table: HasPrimaryKeyColumn>,
{
    type Keys = (PrimaryKeyValue<T::Table>,);
    type ValidationError = <T as InsertBuilder<Conn>>::ValidationError;

    #[inline]
    fn insert_tuple(self, conn: &mut Conn) -> BuilderResult<Self::Keys, Self::ValidationError> {
        self.0.insert_builder_key(conn)
    }
}

impl<Conn, Head, Tail> InsertTuple<Conn> for (Head, Tail)
where
    Head: InsertBuilder<Conn, Table: HasPrimaryKeyColumn>,
    Tail: InsertTuple<Conn>,
{
    type Keys = (PrimaryKeyValue<Head::Table>, Tail::Keys);
    type ValidationError = EitherValidationError<
        <Head as InsertBuilder<Conn>>::ValidationError,
        <Tail as InsertTuple<Conn>>::ValidationError,
    >;

    #[inline]
    fn insert_tuple(self, conn: &mut Conn) -> BuilderResult<Self::Keys, Self::ValidationError> {
        let (head,) = self
            .0
            .insert_builder_key(conn)
            .map_err(|error| error.map_validation(EitherValidationError::Left))?;
        let tail = self
            .1
            .insert_tuple(conn)
            .map_err(|error| error.map_validation(EitherValidationError::Right))?;
        Ok((head, tail))
    }
}

/// Trait defining the insertion of a tuple of optional builders into the
/// database.
trait InsertOptionTuple<Conn> {
    /// The primary keys of the rows the tuple's builders write, `None` for
    /// every absent builder.
    type Keys;
    /// The validation errors of the tuple's optional builders.
    type ValidationError;

    /// Writes the rows of the present builders and returns their primary
    /// keys.
    ///
    /// # Errors
    ///
    /// Returns an error if any insertion fails, if any remaining validation
    /// fails, or if any database constraints are violated.
    fn insert_option_tuple(
        self,
        conn: &mut Conn,
    ) -> BuilderResult<Self::Keys, Self::ValidationError>;
}

impl<Conn> InsertOptionTuple<Conn> for () {
    type Keys = ();
    type ValidationError = std::convert::Infallible;

    #[inline]
    fn insert_option_tuple(self, _conn: &mut Conn) -> BuilderResult<(), Self::ValidationError> {
        Ok(())
    }
}

impl<Conn, T> InsertOptionTuple<Conn> for (Option<T>,)
where
    T: InsertBuilder<Conn, Table: HasPrimaryKeyColumn>,
{
    type Keys = (Option<PrimaryKeyValue<T::Table>>,);
    type ValidationError = <T as InsertBuilder<Conn>>::ValidationError;

    #[inline]
    fn insert_option_tuple(
        self,
        conn: &mut Conn,
    ) -> BuilderResult<Self::Keys, Self::ValidationError> {
        Ok((match self.0 {
            Some(builder) => Some(builder.insert_builder_key(conn)?.0),
            None => None,
        },))
    }
}

impl<Conn, Head, Tail> InsertOptionTuple<Conn> for (Option<Head>, Tail)
where
    Head: InsertBuilder<Conn, Table: HasPrimaryKeyColumn>,
    Tail: InsertOptionTuple<Conn>,
{
    type Keys = (Option<PrimaryKeyValue<Head::Table>>, Tail::Keys);
    type ValidationError = EitherValidationError<
        <Head as InsertBuilder<Conn>>::ValidationError,
        <Tail as InsertOptionTuple<Conn>>::ValidationError,
    >;

    #[inline]
    fn insert_option_tuple(
        self,
        conn: &mut Conn,
    ) -> BuilderResult<Self::Keys, Self::ValidationError> {
        let head = match self.0 {
            Some(builder) => {
                Some(
                    builder
                        .insert_builder_key(conn)
                        .map_err(|error| error.map_validation(EitherValidationError::Left))?
                        .0,
                )
            }
            None => None,
        };
        let tail = self
            .1
            .insert_option_tuple(conn)
            .map_err(|error| error.map_validation(EitherValidationError::Right))?;
        Ok((head, tail))
    }
}
