//! Submodule defining an `HorizontalSameAs` trait for Diesel columns.

use tuplities::prelude::*;
use typenum::Unsigned;

use crate::{
    ColumnTyped, ForeignPrimaryKey, HasPrimaryKeyColumn, NestedBuildableTables, TableExt,
    TypedNestedTuple,
    ancestors::DescendantWithSelf,
    columns::{
        ColumnsCollection, NestedColumns, NestedColumnsCollection, NonEmptyNestedProjection,
        NonEmptyProjection,
    },
    tables::NonCompositePrimaryKeyNestedTables,
};

/// A column type receiving the value of the foreign column of a horizontal
/// same-as relationship, where `None` stands for a `NULL` foreign value.
///
/// A `NOT NULL` column type cannot hold a `NULL` foreign value and is left
/// unset, while a nullable column type copies it.
///
/// ```
/// use diesel_builders::SameAsReceiver;
///
/// assert_eq!(<i32 as SameAsReceiver<i32>>::receive(Some(18)), Some(18));
/// assert_eq!(<i32 as SameAsReceiver<i32>>::receive(None), None);
/// assert_eq!(<Option<i32> as SameAsReceiver<i32>>::receive(Some(18)), Some(Some(18)));
/// assert_eq!(<Option<i32> as SameAsReceiver<i32>>::receive(None), Some(None));
/// ```
pub trait SameAsReceiver<Value>: Sized {
    /// Returns the value the column takes, or `None` to leave it unset.
    fn receive(value: Option<Value>) -> Option<Self>;
}

impl<T> SameAsReceiver<T> for T {
    #[inline]
    fn receive(value: Option<T>) -> Option<T> {
        value
    }
}

impl<T> SameAsReceiver<T> for Option<T> {
    #[inline]
    fn receive(value: Option<T>) -> Option<Option<T>> {
        Some(value)
    }
}

/// Nested foreign columns of a horizontal same-as key whose values fill the
/// nested host columns `Host`.
///
/// Each foreign value converts into the value type of its host column, so a
/// nullable foreign column may feed a `NOT NULL` host column as described by
/// [`SameAsReceiver`].
pub trait NestedSameAsInto<Host: TypedNestedTuple>: TypedNestedTuple {
    /// Converts the foreign column values into the host column values, `None`
    /// leaving a host column unset.
    fn same_as_into(
        values: Self::NestedTupleColumnType,
    ) -> <Host::NestedTupleColumnType as IntoNestedTupleOption>::IntoOptions;

    /// Converts foreign column values that may be unset into the host column
    /// values, `None` leaving a host column unset.
    fn optional_same_as_into(
        values: <Self::NestedTupleColumnType as IntoNestedTupleOption>::IntoOptions,
    ) -> <Host::NestedTupleColumnType as IntoNestedTupleOption>::IntoOptions;
}

/// Converts the value of the foreign column `Foreign` into the value of the
/// host column `Host`, `None` leaving the host column unset.
#[inline]
fn receive_same_as<Foreign, Host>(value: Foreign::ColumnType) -> Option<Host::ColumnType>
where
    Foreign: ColumnTyped<ValueType: Into<Host::ValueType>>,
    Host: ColumnTyped<ColumnType: SameAsReceiver<Host::ValueType>>,
{
    let value: Option<Foreign::ValueType> = value.into();
    Host::ColumnType::receive(value.map(Into::into))
}

impl<Foreign, Host> NestedSameAsInto<(Host,)> for (Foreign,)
where
    Foreign: ColumnTyped<ValueType: Into<Host::ValueType>>,
    Host: ColumnTyped<ColumnType: SameAsReceiver<Host::ValueType>>,
    (Foreign,): TypedNestedTuple<NestedTupleColumnType = (Foreign::ColumnType,)>,
    (Host,): TypedNestedTuple<NestedTupleColumnType = (Host::ColumnType,)>,
{
    #[inline]
    fn same_as_into((value,): (Foreign::ColumnType,)) -> (Option<Host::ColumnType>,) {
        (receive_same_as::<Foreign, Host>(value),)
    }

    #[inline]
    fn optional_same_as_into(
        (value,): (Option<Foreign::ColumnType>,),
    ) -> (Option<Host::ColumnType>,) {
        (value.and_then(receive_same_as::<Foreign, Host>),)
    }
}

impl<Foreign, ForeignTail, Host, HostTail> NestedSameAsInto<(Host, HostTail)>
    for (Foreign, ForeignTail)
where
    Foreign: ColumnTyped<ValueType: Into<Host::ValueType>>,
    Host: ColumnTyped<ColumnType: SameAsReceiver<Host::ValueType>>,
    ForeignTail: NestedSameAsInto<HostTail>,
    HostTail: TypedNestedTuple,
    (Foreign, ForeignTail): TypedNestedTuple<
        NestedTupleColumnType = (Foreign::ColumnType, ForeignTail::NestedTupleColumnType),
    >,
    (Host, HostTail): TypedNestedTuple<
        NestedTupleColumnType = (Host::ColumnType, HostTail::NestedTupleColumnType),
    >,
{
    #[inline]
    fn same_as_into(
        (value, tail): (Foreign::ColumnType, ForeignTail::NestedTupleColumnType),
    ) -> (
        Option<Host::ColumnType>,
        <HostTail::NestedTupleColumnType as IntoNestedTupleOption>::IntoOptions,
    ) {
        (receive_same_as::<Foreign, Host>(value), ForeignTail::same_as_into(tail))
    }

    #[inline]
    fn optional_same_as_into(
        (value, tail): (
            Option<Foreign::ColumnType>,
            <ForeignTail::NestedTupleColumnType as IntoNestedTupleOption>::IntoOptions,
        ),
    ) -> (
        Option<Host::ColumnType>,
        <HostTail::NestedTupleColumnType as IntoNestedTupleOption>::IntoOptions,
    ) {
        (value.and_then(receive_same_as::<Foreign, Host>), ForeignTail::optional_same_as_into(tail))
    }
}

/// A trait for Diesel columns collections that define horizontal same-as
/// relationships.
pub trait HorizontalSameAsColumns<
    Key: HorizontalKey<HostColumns = HostColumns, ForeignColumns = Self>,
    HostColumns: NonEmptyProjection,
>: NonEmptyProjection<Table = Key::ReferencedTable>
{
}

impl<Key, HostColumns, ForeignColumns> HorizontalSameAsColumns<Key, HostColumns> for ForeignColumns
where
    Key: HorizontalKey<HostColumns = HostColumns, ForeignColumns = ForeignColumns>,
    HostColumns: NonEmptyProjection,
    ForeignColumns: NonEmptyProjection<Table = Key::ReferencedTable>,
{
}

/// A trait for Diesel columns that define horizontal same-as relationships.
///
/// Horizontal same-as relationships occur when columns in different tables
/// should maintain the same values due to foreign key relationships.
/// This is commonly used in triangular dependencies where a child table
/// references both a parent and a side table.
///
/// # Type Parameters
///
/// * `HostColumns`: Columns in the current table that mirror values from
///   foreign columns
/// * `ForeignColumns`: Columns in referenced tables that provide the source
///   values
pub trait HorizontalKey:
    ForeignPrimaryKey<ReferencedTable: DescendantWithSelf, Table: HasPrimaryKeyColumn>
{
    /// The set of host columns in the same table which have
    /// an horizontal same-as relationship defined by this key.
    type HostColumns: NonEmptyProjection<Table = Self::Table, Nested: NonEmptyNestedProjection>;
    /// The set of foreign columns in other tables which have
    /// an horizontal same-as relationship defined by this key.
    type ForeignColumns: HorizontalSameAsColumns<
            Self,
            Self::HostColumns,
            Nested: NonEmptyNestedProjection
                        + NestedSameAsInto<<Self::HostColumns as NestTuple>::Nested>,
        >;
}

/// Extension trait for [`HorizontalKey`] to access nested host and foreign
/// columns.
pub trait HorizontalKeyExt: HorizontalKey {
    /// The nested host columns.
    type NestedHostColumns: NonEmptyNestedProjection;
    /// The nested foreign columns.
    type NestedForeignColumns: NonEmptyNestedProjection + NestedSameAsInto<Self::NestedHostColumns>;
}

impl<K> HorizontalKeyExt for K
where
    K: HorizontalKey,
{
    type NestedHostColumns = <K::HostColumns as NestTuple>::Nested;
    type NestedForeignColumns = <K::ForeignColumns as NestTuple>::Nested;
}

/// Index in a tuple for a mandatory same-as relationship.
pub trait MandatorySameAsIndex:
    HorizontalKeyExt<
    NestedHostColumns: NestedTupleStartsWith<<Self::Table as TableExt>::NestedPrimaryKeyColumns>,
>
{
    /// The index in the n-uple of host columns where the mandatory same-as
    /// relationship is defined.
    type Idx: Unsigned;
}

/// Index in a tuple for a discretionary same-as relationship.
pub trait DiscretionarySameAsIndex: HorizontalKeyExt {
    /// The index in the n-uple of host columns where the discretionary same-as
    /// relationship is defined.
    type Idx: Unsigned;
}

/// A trait for Diesel nested columns collections that define horizontal same-as
/// relationships.
pub trait HorizontalNestedKeys<T>: NestedColumns {
    /// The set of referenced tables.
    type NestedReferencedTables: NonCompositePrimaryKeyNestedTables<
            NestedPrimaryKeyColumns: NestedColumns<
                NestedTupleColumnType = <Self as TypedNestedTuple>::NestedTupleColumnType,
            >,
        > + NestedBuildableTables<
            NestedOptionalBuilders: NestedTupleOptionWith<
                &'static str,
                SameDepth = <Self as NestedColumns>::NestedNames,
            >,
        >;
    /// Tuple of tuples of host columns associated to each horizontal same-as
    /// key.
    type NestedHostColumnsMatrix: NestedColumnsCollection<FlattenedMatrix: ColumnsCollection>;
    /// Tuple of tuples of foreign columns associated to each horizontal same-as
    /// key.
    type NestedForeignColumnsMatrix: NestedColumnsCollection<FlattenedMatrix: ColumnsCollection>;
}

impl<T> HorizontalNestedKeys<T> for () {
    type NestedReferencedTables = ();
    type NestedHostColumnsMatrix = ();
    type NestedForeignColumnsMatrix = ();
}

impl<Head, T> HorizontalNestedKeys<T> for (Head,)
where
    T: HasPrimaryKeyColumn,
    Head: HorizontalKeyExt<Table = T>,
    (Head,): NestedColumns,
    (Head::ReferencedTable,): NonCompositePrimaryKeyNestedTables<
            NestedPrimaryKeyColumns: NestedColumns<
                NestedTupleColumnType = <(Head,) as TypedNestedTuple>::NestedTupleColumnType,
            >,
        > + NestedBuildableTables<
            NestedOptionalBuilders: NestedTupleOptionWith<
                &'static str,
                SameDepth = <Self as NestedColumns>::NestedNames,
            >,
        >,
    (Head::NestedHostColumns,): NestedColumnsCollection<FlattenedMatrix: ColumnsCollection>,
    (Head::NestedForeignColumns,): NestedColumnsCollection<FlattenedMatrix: ColumnsCollection>,
{
    type NestedReferencedTables = (Head::ReferencedTable,);
    type NestedHostColumnsMatrix = (Head::NestedHostColumns,);
    type NestedForeignColumnsMatrix = (Head::NestedForeignColumns,);
}

impl<Head, Tail, T> HorizontalNestedKeys<T> for (Head, Tail)
where
    T: HasPrimaryKeyColumn,
    Head: HorizontalKeyExt<Table = T>,
    Tail: HorizontalNestedKeys<T>,
    (Head, Tail):
        NestedColumns<NestedTupleColumnType = (Head::ColumnType, Tail::NestedTupleColumnType)>,
    (Head::ReferencedTable, Tail::NestedReferencedTables): NonCompositePrimaryKeyNestedTables<
            NestedPrimaryKeyColumns: NestedColumns<
                NestedTupleColumnType = (Head::ColumnType, Tail::NestedTupleColumnType),
            >,
        > + NestedBuildableTables<
            NestedOptionalBuilders: NestedTupleOptionWith<
                &'static str,
                SameDepth = <Self as NestedColumns>::NestedNames,
            >,
        >,
    (Head::NestedHostColumns, Tail::NestedHostColumnsMatrix):
        NestedColumnsCollection<FlattenedMatrix: ColumnsCollection>,
    (Head::NestedForeignColumns, Tail::NestedForeignColumnsMatrix):
        NestedColumnsCollection<FlattenedMatrix: ColumnsCollection>,
{
    type NestedReferencedTables = (Head::ReferencedTable, Tail::NestedReferencedTables);
    type NestedHostColumnsMatrix = (Head::NestedHostColumns, Tail::NestedHostColumnsMatrix);
    type NestedForeignColumnsMatrix =
        (Head::NestedForeignColumns, Tail::NestedForeignColumnsMatrix);
}
