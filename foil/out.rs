#![feature(prelude_import)]
#![feature(generic_associated_types)]
#![allow(unstable_name_collisions)]
#![warn(clippy::pedantic)]
#![forbid(unused_must_use)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
pub use entity::{Create, Delete, Entity, Field, Update};
pub use manager::Manager;
pub mod entity {
    use crate::manager::{
        CountQuery, DeleteQuery, FindOperator, FromRecord, InsertQuery, InsertReturningQuery,
        IntoInputRecord, IntoSelector, Manager, OrderBy, Record, RecordError, SelectQuery,
        Selector, UpdateQuery, Value,
    };
    use futures::{
        future::BoxFuture, stream::BoxStream, FutureExt, StreamExt, TryFutureExt, TryStreamExt,
    };
    use sqlx::Database;
    use std::error::Error;
    use std::marker::PhantomData;
    use thiserror::Error;
    pub trait Entity<DB: Database>: FromRecord<DB> + 'static {
        type Col: Col + Send;
        type Id: for<'q> Value<'q, DB> + Send;
        type Selector<'q>: IntoSelector<'q, DB> + Send;
        fn table_name() -> &'static str;
        fn col_names() -> &'static [&'static str];
        fn id_col_name() -> &'static str;
        fn id(&self) -> Self::Id;
        fn get<'m, M: Manager<'m, DB>>(
            manager: M,
            id: Self::Id,
        ) -> BoxFuture<'m, Result<Self, SelectOneError<M::Error>>> {
            let mut selector = Selector::new();
            selector.add_col(Self::id_col_name(), FindOperator::Eq(Box::new(id)));
            Box::pin(
                Selection::<_, Self, DB>::new(manager.select(SelectQuery {
                    table_name: Self::table_name(),
                    col_names: Self::col_names(),
                    selectors: <[_]>::into_vec(box [selector]),
                    order_by: None,
                    offset: None,
                    limit: None,
                }))
                .one(),
            )
        }
        fn find<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            manager: M,
            selectors: Vec<Self::Selector<'q>>,
        ) -> Selection<'o, M::Error, Self, DB> {
            Self::find_with_options(
                manager,
                selectors,
                FindOptions {
                    order_by: None,
                    offset: None,
                    limit: None,
                },
            )
        }
        fn find_with_options<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            manager: M,
            selectors: Vec<Self::Selector<'q>>,
            options: FindOptions<Self::Col>,
        ) -> Selection<'o, M::Error, Self, DB> {
            Selection::new(
                manager.select(SelectQuery {
                    table_name: Self::table_name(),
                    col_names: Self::col_names(),
                    selectors: selectors
                        .into_iter()
                        .map(IntoSelector::into_selector)
                        .collect(),
                    order_by: options.order_by.map(|order_by| OrderBy {
                        order: order_by.order,
                        cols: order_by.cols.iter().map(Col::as_str).collect(),
                    }),
                    offset: options.offset,
                    limit: options.limit,
                }),
            )
        }
        fn count<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            manager: M,
            selectors: Vec<Self::Selector<'q>>,
        ) -> BoxFuture<'o, Result<u32, M::Error>>
        where
            for<'a> u32: sqlx::Type<DB> + sqlx::Decode<'a, DB>,
            for<'a> &'a str: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>,
        {
            manager.count(CountQuery {
                table_name: Self::table_name(),
                selectors: selectors
                    .into_iter()
                    .map(IntoSelector::into_selector)
                    .collect(),
            })
        }
        fn exists<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            manager: M,
            selectors: Vec<Self::Selector<'q>>,
        ) -> BoxFuture<'o, Result<bool, M::Error>>
        where
            for<'a> u32: sqlx::Type<DB> + sqlx::Decode<'a, DB>,
            for<'a> &'a str: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>,
        {
            Box::pin(Self::count(manager, selectors).map_ok(|count| count != 0))
        }
    }
    pub struct FindOptions<C> {
        pub order_by: Option<OrderBy<C>>,
        pub offset: Option<u32>,
        pub limit: Option<u32>,
    }
    pub trait Create<DB: Database>: Entity<DB> + Send {
        type Input<'q>: From<&'q Self> + IntoInputRecord<'q, DB> + Send + Sync;
        fn generated_col_names() -> &'static [&'static str];
        fn construct<'q>(
            input: &Self::Input<'q>,
            generated: &Record<DB>,
        ) -> Result<Self, RecordError>;
        fn create<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            manager: M,
            input: Self::Input<'q>,
        ) -> BoxFuture<'o, Result<Self, CreateError<M::Error>>> {
            Box::pin(
                Self::create_many(manager, <[_]>::into_vec(box [input]))
                    .map_ok(|mut many| many.pop().unwrap()),
            )
        }
        fn create_many<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            manager: M,
            inputs: Vec<Self::Input<'q>>,
        ) -> BoxFuture<'o, Result<Vec<Self>, CreateError<M::Error>>> {
            Box::pin(
                manager
                    .insert_returning(InsertReturningQuery {
                        insert_query: InsertQuery {
                            table_name: Self::table_name(),
                            col_names: Self::col_names(),
                            values: inputs
                                .iter()
                                .map(IntoInputRecord::to_input_record)
                                .collect::<Vec<_>>(),
                        },
                        returning_cols: Self::generated_col_names(),
                    })
                    .try_collect::<Vec<_>>()
                    .map(move |result| match result {
                        Ok(records) => Ok(records
                            .iter()
                            .enumerate()
                            .map(|(index, record)| {
                                Ok::<_, CreateError<_>>(Self::construct(
                                    inputs.get(index).ok_or(CreateError::WrongNumberOfRows)?,
                                    record,
                                )?)
                            })
                            .collect::<Result<Vec<_>, _>>()?),
                        Err(err) => Err(CreateError::Manager(err)),
                    }),
            )
        }
        fn persist<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            &'q self,
            manager: M,
        ) -> BoxFuture<'o, Result<(), M::Error>> {
            Self::insert(manager, <[_]>::into_vec(box [self.into()]))
        }
        fn insert<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            manager: M,
            inputs: Vec<Self::Input<'q>>,
        ) -> BoxFuture<'o, Result<(), M::Error>> {
            manager.insert(InsertQuery {
                table_name: Self::table_name(),
                col_names: Self::col_names(),
                values: inputs
                    .into_iter()
                    .map(IntoInputRecord::into_input_record)
                    .collect(),
            })
        }
    }
    pub enum CreateError<E: Error + Send + Sync> {
        #[error(transparent)]
        Manager(E),
        #[error(transparent)]
        Record(#[from] RecordError),
        #[error("number of rows returned by query doesn't match number of inputs")]
        WrongNumberOfRows,
    }
    #[automatically_derived]
    #[allow(unused_qualifications)]
    impl<E: ::core::fmt::Debug + Error + Send + Sync> ::core::fmt::Debug for CreateError<E> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            match (&*self,) {
                (&CreateError::Manager(ref __self_0),) => {
                    let debug_trait_builder =
                        &mut ::core::fmt::Formatter::debug_tuple(f, "Manager");
                    let _ = ::core::fmt::DebugTuple::field(debug_trait_builder, &&(*__self_0));
                    ::core::fmt::DebugTuple::finish(debug_trait_builder)
                }
                (&CreateError::Record(ref __self_0),) => {
                    let debug_trait_builder = &mut ::core::fmt::Formatter::debug_tuple(f, "Record");
                    let _ = ::core::fmt::DebugTuple::field(debug_trait_builder, &&(*__self_0));
                    ::core::fmt::DebugTuple::finish(debug_trait_builder)
                }
                (&CreateError::WrongNumberOfRows,) => {
                    ::core::fmt::Formatter::write_str(f, "WrongNumberOfRows")
                }
            }
        }
    }
    #[allow(unused_qualifications)]
    impl<E: Error + Send + Sync> std::error::Error for CreateError<E>
    where
        E: std::error::Error,
        Self: std::fmt::Debug + std::fmt::Display,
    {
        fn source(&self) -> std::option::Option<&(dyn std::error::Error + 'static)> {
            use thiserror::private::AsDynError;
            #[allow(deprecated)]
            match self {
                CreateError::Manager { 0: transparent } => {
                    std::error::Error::source(transparent.as_dyn_error())
                }
                CreateError::Record { 0: transparent } => {
                    std::error::Error::source(transparent.as_dyn_error())
                }
                CreateError::WrongNumberOfRows { .. } => std::option::Option::None,
            }
        }
    }
    #[allow(unused_qualifications)]
    impl<E: Error + Send + Sync> std::fmt::Display for CreateError<E>
    where
        E: std::fmt::Display,
    {
        fn fmt(&self, __formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            #[allow(unused_variables, deprecated, clippy::used_underscore_binding)]
            match self {
                CreateError::Manager(_0) => std::fmt::Display::fmt(_0, __formatter),
                CreateError::Record(_0) => std::fmt::Display::fmt(_0, __formatter),
                CreateError::WrongNumberOfRows {} => {
                    __formatter.write_fmt(::core::fmt::Arguments::new_v1(
                        &["number of rows returned by query doesn\'t match number of inputs"],
                        &match () {
                            () => [],
                        },
                    ))
                }
            }
        }
    }
    #[allow(unused_qualifications)]
    impl<E: Error + Send + Sync> std::convert::From<RecordError> for CreateError<E> {
        #[allow(deprecated)]
        fn from(source: RecordError) -> Self {
            CreateError::Record { 0: source }
        }
    }
    pub trait Update<DB: Database>: Entity<DB> + Send {
        type Patch<'q>: IntoInputRecord<'q, DB> + Send + Sync;
        fn apply_patch(&mut self, patch: Self::Patch<'_>);
        fn patch<'m: 'o, 'q: 'o, 'e: 'o, 'o, M: Manager<'m, DB>>(
            &'e mut self,
            manager: M,
            patch: Self::Patch<'q>,
        ) -> BoxFuture<'o, Result<(), M::Error>> {
            let mut selector = Selector::new();
            selector.add_col(
                Self::id_col_name(),
                FindOperator::Eq(Box::new(self.id()) as Box<dyn Value<'q, _>>),
            );
            Box::pin(
                manager
                    .update(UpdateQuery {
                        table_name: Self::table_name(),
                        selectors: <[_]>::into_vec(box [selector]),
                        new_values: patch.to_input_record(),
                    })
                    .and_then(|_| async {
                        self.apply_patch(patch);
                        Ok(())
                    }),
            )
        }
        fn update<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            manager: M,
            selectors: Vec<Self::Selector<'q>>,
            patch: Self::Patch<'q>,
        ) -> BoxFuture<'o, Result<(), M::Error>> {
            manager.update(UpdateQuery {
                table_name: Self::table_name(),
                selectors: selectors
                    .into_iter()
                    .map(IntoSelector::into_selector)
                    .collect(),
                new_values: patch.into_input_record(),
            })
        }
    }
    pub trait Delete<DB: Database>: Entity<DB> {
        fn remove<'m, M: Manager<'m, DB>>(
            &self,
            manager: M,
        ) -> BoxFuture<'m, Result<(), M::Error>> {
            let mut selector = Selector::new();
            selector.add_col(Self::id_col_name(), FindOperator::Eq(Box::new(self.id())));
            manager.delete(DeleteQuery {
                table_name: Self::table_name(),
                selectors: <[_]>::into_vec(box [selector]),
            })
        }
        fn delete<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
            manager: M,
            selectors: Vec<Self::Selector<'q>>,
        ) -> BoxFuture<'o, Result<(), M::Error>> {
            manager.delete(DeleteQuery {
                table_name: Self::table_name(),
                selectors: selectors
                    .into_iter()
                    .map(IntoSelector::into_selector)
                    .collect(),
            })
        }
    }
    pub trait Col: Copy {
        fn as_str(&self) -> &'static str;
    }
    pub enum Field<T> {
        Set(T),
        Omit,
    }
    pub struct Selection<'m, E: Error + Send + Sync, T: FromRecord<DB>, DB: Database> {
        stream: BoxStream<'m, Result<Record<DB>, E>>,
        marker: PhantomData<fn() -> T>,
    }
    pub enum SelectError<E: Error + Send + Sync> {
        #[error(transparent)]
        Manager(E),
        #[error(transparent)]
        Record(#[from] RecordError),
    }
    #[automatically_derived]
    #[allow(unused_qualifications)]
    impl<E: ::core::fmt::Debug + Error + Send + Sync> ::core::fmt::Debug for SelectError<E> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            match (&*self,) {
                (&SelectError::Manager(ref __self_0),) => {
                    let debug_trait_builder =
                        &mut ::core::fmt::Formatter::debug_tuple(f, "Manager");
                    let _ = ::core::fmt::DebugTuple::field(debug_trait_builder, &&(*__self_0));
                    ::core::fmt::DebugTuple::finish(debug_trait_builder)
                }
                (&SelectError::Record(ref __self_0),) => {
                    let debug_trait_builder = &mut ::core::fmt::Formatter::debug_tuple(f, "Record");
                    let _ = ::core::fmt::DebugTuple::field(debug_trait_builder, &&(*__self_0));
                    ::core::fmt::DebugTuple::finish(debug_trait_builder)
                }
            }
        }
    }
    #[allow(unused_qualifications)]
    impl<E: Error + Send + Sync> std::error::Error for SelectError<E>
    where
        E: std::error::Error,
        Self: std::fmt::Debug + std::fmt::Display,
    {
        fn source(&self) -> std::option::Option<&(dyn std::error::Error + 'static)> {
            use thiserror::private::AsDynError;
            #[allow(deprecated)]
            match self {
                SelectError::Manager { 0: transparent } => {
                    std::error::Error::source(transparent.as_dyn_error())
                }
                SelectError::Record { 0: transparent } => {
                    std::error::Error::source(transparent.as_dyn_error())
                }
            }
        }
    }
    #[allow(unused_qualifications)]
    impl<E: Error + Send + Sync> std::fmt::Display for SelectError<E>
    where
        E: std::fmt::Display,
    {
        fn fmt(&self, __formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            #[allow(unused_variables, deprecated, clippy::used_underscore_binding)]
            match self {
                SelectError::Manager(_0) => std::fmt::Display::fmt(_0, __formatter),
                SelectError::Record(_0) => std::fmt::Display::fmt(_0, __formatter),
            }
        }
    }
    #[allow(unused_qualifications)]
    impl<E: Error + Send + Sync> std::convert::From<RecordError> for SelectError<E> {
        #[allow(deprecated)]
        fn from(source: RecordError) -> Self {
            SelectError::Record { 0: source }
        }
    }
    pub enum SelectOneError<E: Error + Send + Sync> {
        #[error(transparent)]
        Select(#[from] SelectError<E>),
        #[error("query returned less rows than expected")]
        RowNotFound,
    }
    #[automatically_derived]
    #[allow(unused_qualifications)]
    impl<E: ::core::fmt::Debug + Error + Send + Sync> ::core::fmt::Debug for SelectOneError<E> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            match (&*self,) {
                (&SelectOneError::Select(ref __self_0),) => {
                    let debug_trait_builder = &mut ::core::fmt::Formatter::debug_tuple(f, "Select");
                    let _ = ::core::fmt::DebugTuple::field(debug_trait_builder, &&(*__self_0));
                    ::core::fmt::DebugTuple::finish(debug_trait_builder)
                }
                (&SelectOneError::RowNotFound,) => {
                    ::core::fmt::Formatter::write_str(f, "RowNotFound")
                }
            }
        }
    }
    #[allow(unused_qualifications)]
    impl<E: Error + Send + Sync> std::error::Error for SelectOneError<E>
    where
        SelectError<E>: std::error::Error,
        Self: std::fmt::Debug + std::fmt::Display,
    {
        fn source(&self) -> std::option::Option<&(dyn std::error::Error + 'static)> {
            use thiserror::private::AsDynError;
            #[allow(deprecated)]
            match self {
                SelectOneError::Select { 0: transparent } => {
                    std::error::Error::source(transparent.as_dyn_error())
                }
                SelectOneError::RowNotFound { .. } => std::option::Option::None,
            }
        }
    }
    #[allow(unused_qualifications)]
    impl<E: Error + Send + Sync> std::fmt::Display for SelectOneError<E>
    where
        SelectError<E>: std::fmt::Display,
    {
        fn fmt(&self, __formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            #[allow(unused_variables, deprecated, clippy::used_underscore_binding)]
            match self {
                SelectOneError::Select(_0) => std::fmt::Display::fmt(_0, __formatter),
                SelectOneError::RowNotFound {} => {
                    __formatter.write_fmt(::core::fmt::Arguments::new_v1(
                        &["query returned less rows than expected"],
                        &match () {
                            () => [],
                        },
                    ))
                }
            }
        }
    }
    #[allow(unused_qualifications)]
    impl<E: Error + Send + Sync> std::convert::From<SelectError<E>> for SelectOneError<E> {
        #[allow(deprecated)]
        fn from(source: SelectError<E>) -> Self {
            SelectOneError::Select { 0: source }
        }
    }
    impl<'m, T: FromRecord<DB>, DB: Database, E: Error + Send + Sync + 'm> Selection<'m, E, T, DB> {
        fn new(stream: BoxStream<'m, Result<Record<DB>, E>>) -> Self {
            Self {
                stream,
                marker: PhantomData,
            }
        }
        pub async fn optional(mut self) -> Result<Option<T>, SelectError<E>> {
            self.stream
                .try_next()
                .await
                .map_err(SelectError::Manager)?
                .map(|record| T::from_record(&record).map_err(Into::into))
                .transpose()
        }
        pub async fn one(self) -> Result<T, SelectOneError<E>> {
            self.optional().await?.ok_or(SelectOneError::RowNotFound)
        }
        #[allow(clippy::must_use_candidate)]
        pub fn stream(self) -> BoxStream<'m, Result<T, SelectError<E>>> {
            Box::pin(self.stream.map(|result| match result {
                Ok(record) => Ok(T::from_record(&record)?),
                Err(err) => Err(SelectError::Manager(err)),
            }))
        }
        pub async fn all(self) -> Result<Vec<T>, SelectError<E>> {
            self.stream().try_collect().await
        }
    }
}
pub mod manager {
    use futures::{future::BoxFuture, stream::BoxStream};
    use sqlx::{Database, Decode, Row, Type};
    use std::any::Any;
    use std::collections::BTreeMap;
    use std::error::Error;
    use thiserror::Error;
    mod display {
        use super::{
            CountQuery, DeleteQuery, FindOperator, InsertQuery, InsertReturningQuery, Order,
            SelectQuery, Selector, UpdateQuery, Value,
        };
        use sqlx::Database;
        use std::fmt::{Display, Formatter, Result};
        impl<DB: Database + WithBindParameters> Display for SelectQuery<'_, DB> {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &["SELECT "],
                    &match () {
                        () => [],
                    },
                ))?;
                format_list(self.col_names.iter(), f, |col_name, f| {
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &["\"", "\""],
                        &match (&col_name,) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    ))
                })?;
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &[" FROM \"", "\""],
                    &match (&self.table_name,) {
                        (arg0,) => [::core::fmt::ArgumentV1::new(
                            arg0,
                            ::core::fmt::Display::fmt,
                        )],
                    },
                ))?;
                format_selectors(&self.selectors, &mut DB::parameter_factory(), f)?;
                if let Some(order_by) = &self.order_by {
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &[" ORDER BY "],
                        &match () {
                            () => [],
                        },
                    ))?;
                    format_list(order_by.cols.iter(), f, |col_name, f| {
                        f.write_fmt(::core::fmt::Arguments::new_v1(
                            &["\"", "\""],
                            &match (&col_name,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        ))
                    })?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &[" "],
                        &match (&order_by.order,) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    ))?;
                }
                if let Some(offset) = self.offset {
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &[" SKIP "],
                        &match (&offset,) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    ))?;
                }
                if let Some(limit) = self.limit {
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &[" TAKE "],
                        &match (&limit,) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    ))?;
                }
                Ok(())
            }
        }
        impl<DB: Database + WithBindParameters> Display for CountQuery<'_, DB> {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &["SELECT COUNT (*) AS \"cnt\" FROM \"", "\""],
                    &match (&self.table_name,) {
                        (arg0,) => [::core::fmt::ArgumentV1::new(
                            arg0,
                            ::core::fmt::Display::fmt,
                        )],
                    },
                ))?;
                format_selectors(&self.selectors, &mut DB::parameter_factory(), f)?;
                Ok(())
            }
        }
        impl<DB: Database + WithBindParameters> Display for InsertQuery<'_, DB> {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &["INSERT INTO \"", "\" ("],
                    &match (&self.table_name,) {
                        (arg0,) => [::core::fmt::ArgumentV1::new(
                            arg0,
                            ::core::fmt::Display::fmt,
                        )],
                    },
                ))?;
                format_list(self.col_names.iter(), f, |col_name, f| {
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &["\"", "\""],
                        &match (&col_name,) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    ))
                })?;
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &[") VALUES "],
                    &match () {
                        () => [],
                    },
                ))?;
                let mut parameter_factory = DB::parameter_factory();
                format_list(self.values.iter(), f, |values, f| {
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &["("],
                        &match () {
                            () => [],
                        },
                    ))?;
                    format_list(self.col_names.iter(), f, |col_name, f| {
                        if values.has_col(col_name) {
                            f.write_fmt(::core::fmt::Arguments::new_v1(
                                &[""],
                                &match (&parameter_factory.get(),) {
                                    (arg0,) => [::core::fmt::ArgumentV1::new(
                                        arg0,
                                        ::core::fmt::Display::fmt,
                                    )],
                                },
                            ))?;
                        } else {
                            f.write_fmt(::core::fmt::Arguments::new_v1(
                                &["DEFAULT"],
                                &match () {
                                    () => [],
                                },
                            ))?;
                        }
                        Ok(())
                    })?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &[")"],
                        &match () {
                            () => [],
                        },
                    ))?;
                    Ok(())
                })?;
                Ok(())
            }
        }
        impl<DB: Database + WithBindParameters> Display for InsertReturningQuery<'_, DB> {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &[""],
                    &match (&self.insert_query,) {
                        (arg0,) => [::core::fmt::ArgumentV1::new(
                            arg0,
                            ::core::fmt::Display::fmt,
                        )],
                    },
                ))?;
                if self.returning_cols.is_empty() {
                    return Ok(());
                }
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &[" RETURNING "],
                    &match () {
                        () => [],
                    },
                ))?;
                format_list(self.returning_cols.iter(), f, |col_name, f| {
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &["\"", "\""],
                        &match (&col_name,) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    ))
                })?;
                Ok(())
            }
        }
        impl<DB: Database + WithBindParameters> Display for UpdateQuery<'_, DB> {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &["UPDATE \"", "\" SET "],
                    &match (&self.table_name,) {
                        (arg0,) => [::core::fmt::ArgumentV1::new(
                            arg0,
                            ::core::fmt::Display::fmt,
                        )],
                    },
                ))?;
                let mut parameter_factory = DB::parameter_factory();
                let cols = self.new_values.cols();
                format_list(cols, f, |(col_name, _), f| {
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &["\"", "\" = "],
                        &match (&col_name, &parameter_factory.get()) {
                            (arg0, arg1) => [
                                ::core::fmt::ArgumentV1::new(arg0, ::core::fmt::Display::fmt),
                                ::core::fmt::ArgumentV1::new(arg1, ::core::fmt::Display::fmt),
                            ],
                        },
                    ))
                })?;
                format_selectors(&self.selectors, &mut parameter_factory, f)?;
                Ok(())
            }
        }
        impl<DB: Database + WithBindParameters> Display for DeleteQuery<'_, DB> {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &["DELETE FROM \"", "\""],
                    &match (&self.table_name,) {
                        (arg0,) => [::core::fmt::ArgumentV1::new(
                            arg0,
                            ::core::fmt::Display::fmt,
                        )],
                    },
                ))?;
                format_selectors(&self.selectors, &mut DB::parameter_factory(), f)
            }
        }
        impl Display for Order {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                f.write_str(match self {
                    Self::Asc => "ASC",
                    Self::Desc => "DESC",
                })
            }
        }
        fn format_selectors<DB: Database + WithBindParameters, W: std::fmt::Write>(
            selectors: &[Selector<DB>],
            parameter_factory: &mut DB::ParameterFactory,
            w: &mut W,
        ) -> Result {
            if selectors.len() == 1 && selectors[0].is_empty() {
                return Ok(());
            }
            w.write_fmt(::core::fmt::Arguments::new_v1(
                &[" WHERE "],
                &match () {
                    () => [],
                },
            ))?;
            match selectors.len() {
                0 => w.write_fmt(::core::fmt::Arguments::new_v1(
                    &["<empty list>"],
                    &match () {
                        () => [],
                    },
                ))?,
                1 => format_selector(selectors.first().unwrap(), parameter_factory, w)?,
                _ => {
                    for (index, selector) in selectors.iter().enumerate() {
                        w.write_fmt(::core::fmt::Arguments::new_v1(
                            &["("],
                            &match () {
                                () => [],
                            },
                        ))?;
                        format_selector(selector, parameter_factory, w)?;
                        w.write_fmt(::core::fmt::Arguments::new_v1(
                            &[")"],
                            &match () {
                                () => [],
                            },
                        ))?;
                        if index != selectors.len() - 1 {
                            w.write_fmt(::core::fmt::Arguments::new_v1(
                                &[" OR "],
                                &match () {
                                    () => [],
                                },
                            ))?;
                        }
                    }
                }
            }
            Ok(())
        }
        fn format_selector<DB: Database + WithBindParameters, W: std::fmt::Write>(
            selector: &Selector<DB>,
            parameter_factory: &mut DB::ParameterFactory,
            f: &mut W,
        ) -> Result {
            let mut format_col =
                |f: &mut W, (col_name, op): (&str, &FindOperator<Box<dyn Value<DB>>>)| match op {
                    FindOperator::Eq(value) => {
                        if value.is_null() {
                            f.write_fmt(::core::fmt::Arguments::new_v1(
                                &["\"", "\" IS NULL"],
                                &match (&col_name,) {
                                    (arg0,) => [::core::fmt::ArgumentV1::new(
                                        arg0,
                                        ::core::fmt::Display::fmt,
                                    )],
                                },
                            ))
                        } else {
                            f.write_fmt(::core::fmt::Arguments::new_v1(
                                &["\"", "\" = "],
                                &match (&col_name, &parameter_factory.get()) {
                                    (arg0, arg1) => [
                                        ::core::fmt::ArgumentV1::new(
                                            arg0,
                                            ::core::fmt::Display::fmt,
                                        ),
                                        ::core::fmt::ArgumentV1::new(
                                            arg1,
                                            ::core::fmt::Display::fmt,
                                        ),
                                    ],
                                },
                            ))
                        }
                    }
                    FindOperator::Ne(value) => {
                        if value.is_null() {
                            f.write_fmt(::core::fmt::Arguments::new_v1(
                                &["\"", "\" IS NOT NULL"],
                                &match (&col_name,) {
                                    (arg0,) => [::core::fmt::ArgumentV1::new(
                                        arg0,
                                        ::core::fmt::Display::fmt,
                                    )],
                                },
                            ))
                        } else {
                            f.write_fmt(::core::fmt::Arguments::new_v1(
                                &["\"", "\" != "],
                                &match (&col_name, &parameter_factory.get()) {
                                    (arg0, arg1) => [
                                        ::core::fmt::ArgumentV1::new(
                                            arg0,
                                            ::core::fmt::Display::fmt,
                                        ),
                                        ::core::fmt::ArgumentV1::new(
                                            arg1,
                                            ::core::fmt::Display::fmt,
                                        ),
                                    ],
                                },
                            ))
                        }
                    }
                    FindOperator::In(values) => {
                        f.write_fmt(::core::fmt::Arguments::new_v1(
                            &["\"", "\" IN ("],
                            &match (&col_name,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        ))?;
                        format_list(
                            values
                                .iter()
                                .filter_map(|value| {
                                    if value.is_null() {
                                        None
                                    } else {
                                        Some(parameter_factory.get())
                                    }
                                })
                                .collect::<Vec<_>>()
                                .iter(),
                            f,
                            |parameter, f| {
                                f.write_fmt(::core::fmt::Arguments::new_v1(
                                    &[""],
                                    &match (&parameter,) {
                                        (arg0,) => [::core::fmt::ArgumentV1::new(
                                            arg0,
                                            ::core::fmt::Display::fmt,
                                        )],
                                    },
                                ))
                            },
                        )?;
                        f.write_fmt(::core::fmt::Arguments::new_v1(
                            &[")"],
                            &match () {
                                () => [],
                            },
                        ))?;
                        if values.iter().any(|value| value.is_null()) {
                            f.write_fmt(::core::fmt::Arguments::new_v1(
                                &[" OR \"", "\" IS NULL"],
                                &match (&col_name,) {
                                    (arg0,) => [::core::fmt::ArgumentV1::new(
                                        arg0,
                                        ::core::fmt::Display::fmt,
                                    )],
                                },
                            ))?;
                        }
                        Ok(())
                    }
                    FindOperator::NotIn(values) => {
                        f.write_fmt(::core::fmt::Arguments::new_v1(
                            &["\"", "\" NOT IN ("],
                            &match (&col_name,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        ))?;
                        format_list(
                            values
                                .iter()
                                .filter_map(|value| {
                                    if value.is_null() {
                                        None
                                    } else {
                                        Some(parameter_factory.get())
                                    }
                                })
                                .collect::<Vec<_>>()
                                .iter(),
                            f,
                            |parameter, f| {
                                f.write_fmt(::core::fmt::Arguments::new_v1(
                                    &[""],
                                    &match (&parameter,) {
                                        (arg0,) => [::core::fmt::ArgumentV1::new(
                                            arg0,
                                            ::core::fmt::Display::fmt,
                                        )],
                                    },
                                ))
                            },
                        )?;
                        f.write_fmt(::core::fmt::Arguments::new_v1(
                            &[")"],
                            &match () {
                                () => [],
                            },
                        ))?;
                        if values.iter().any(|value| value.is_null()) {
                            f.write_fmt(::core::fmt::Arguments::new_v1(
                                &[" AND \"", "\" IS NOT NULL"],
                                &match (&col_name,) {
                                    (arg0,) => [::core::fmt::ArgumentV1::new(
                                        arg0,
                                        ::core::fmt::Display::fmt,
                                    )],
                                },
                            ))?;
                        }
                        Ok(())
                    }
                };
            match selector.len() {
                0 => f.write_fmt(::core::fmt::Arguments::new_v1(
                    &["<empty list>"],
                    &match () {
                        () => [],
                    },
                ))?,
                1 => format_col(f, selector.cols().next().unwrap())?,
                _ => {
                    for (index, col) in selector.cols().enumerate() {
                        f.write_fmt(::core::fmt::Arguments::new_v1(
                            &["("],
                            &match () {
                                () => [],
                            },
                        ))?;
                        format_col(f, col)?;
                        f.write_fmt(::core::fmt::Arguments::new_v1(
                            &[")"],
                            &match () {
                                () => [],
                            },
                        ))?;
                        if index != selector.cols().len() - 1 {
                            f.write_fmt(::core::fmt::Arguments::new_v1(
                                &[" AND "],
                                &match () {
                                    () => [],
                                },
                            ))?;
                        }
                    }
                }
            }
            Ok(())
        }
        fn format_list<T, W: std::fmt::Write>(
            list: impl ExactSizeIterator<Item = T>,
            f: &mut W,
            mut format_fn: impl FnMut(T, &mut W) -> Result,
        ) -> Result {
            let len = list.len();
            if len == 0 {
                f.write_fmt(::core::fmt::Arguments::new_v1(
                    &["<empty list>"],
                    &match () {
                        () => [],
                    },
                ))?;
                return Ok(());
            }
            for (index, item) in list.enumerate() {
                format_fn(item, f)?;
                if index != len - 1 {
                    f.write_fmt(::core::fmt::Arguments::new_v1(
                        &[", "],
                        &match () {
                            () => [],
                        },
                    ))?;
                }
            }
            Ok(())
        }
        pub trait WithBindParameters {
            type ParameterFactory: ParameterFactory;
            fn parameter_factory() -> Self::ParameterFactory {
                Self::ParameterFactory::default()
            }
        }
        pub trait ParameterFactory: Default {
            fn get(&mut self) -> String;
        }
        #[cfg(feature = "postgres")]
        mod pg_parameters {
            use super::{ParameterFactory, WithBindParameters};
            impl WithBindParameters for sqlx::Postgres {
                type ParameterFactory = PgParameterFactory;
            }
            pub struct PgParameterFactory(usize);
            #[automatically_derived]
            #[allow(unused_qualifications)]
            impl ::core::default::Default for PgParameterFactory {
                #[inline]
                fn default() -> PgParameterFactory {
                    PgParameterFactory(::core::default::Default::default())
                }
            }
            impl ParameterFactory for PgParameterFactory {
                fn get(&mut self) -> String {
                    self.0 += 1;
                    {
                        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
                            &["$"],
                            &match (&self.0,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        ));
                        res
                    }
                }
            }
        }
        #[cfg(any(feature = "mysql", feature = "sqlite", feature = "any"))]
        mod unordered_parameters {
            use super::{ParameterFactory, WithBindParameters};
            #[cfg(feature = "mysql")]
            impl WithBindParameters for sqlx::MySql {
                type ParameterFactory = UnorderedParameterFactory;
            }
            #[cfg(feature = "sqlite")]
            impl WithBindParameters for sqlx::Sqlite {
                type ParameterFactory = UnorderedParameterFactory;
            }
            #[cfg(feature = "any")]
            impl WithBindParameters for sqlx::Any {
                type ParameterFactory = UnorderedParameterFactory;
            }
            pub struct UnorderedParameterFactory;
            #[automatically_derived]
            #[allow(unused_qualifications)]
            impl ::core::default::Default for UnorderedParameterFactory {
                #[inline]
                fn default() -> UnorderedParameterFactory {
                    UnorderedParameterFactory {}
                }
            }
            impl ParameterFactory for UnorderedParameterFactory {
                fn get(&mut self) -> String {
                    "?".into()
                }
            }
        }
        #[cfg(feature = "mssql")]
        mod mssql_parameters {
            use super::{ParameterFactory, WithBindParameters};
            impl WithBindParameters for sqlx::Mssql {
                type ParameterFactory = MssqlParameterFactory;
            }
            pub struct MssqlParameterFactory(usize);
            #[automatically_derived]
            #[allow(unused_qualifications)]
            impl ::core::default::Default for MssqlParameterFactory {
                #[inline]
                fn default() -> MssqlParameterFactory {
                    MssqlParameterFactory(::core::default::Default::default())
                }
            }
            impl ParameterFactory for MssqlParameterFactory {
                fn get(&mut self) -> String {
                    self.0 += 1;
                    {
                        let res = ::alloc::fmt::format(::core::fmt::Arguments::new_v1(
                            &["@P"],
                            &match (&self.0,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        ));
                        res
                    }
                }
            }
        }
    }
    pub mod impls {
        mod executor {
            use sqlx::{database::HasArguments, Database, Decode, Executor, Postgres, Row, Type};
            use crate::{
                manager::{
                    CountQuery, DeleteQuery, FindOperator, InputRecord, Record, SelectQuery,
                    Selector,
                },
                Manager,
            };
            impl<'m, T> Manager<'m, Postgres> for T
            where
                T: Executor<'m, Database = Postgres> + 'm,
            {
                type Error = sqlx::Error;
                fn select<'q, 'o>(
                    self,
                    query: crate::manager::SelectQuery<'q, Postgres>,
                ) -> futures::stream::BoxStream<'o, sqlx::Result<crate::manager::Record<Postgres>>>
                where
                    'm: 'o,
                    'q: 'o,
                {
                    if query.col_names.is_empty() {
                        Box::pin(futures::stream::once(async { Ok(Record::new()) }))
                    } else if query.selectors.is_empty() {
                        Box::pin(futures::stream::empty())
                    } else {
                        Box::pin({
                            let (mut __yield_tx, __yield_rx) = ::async_stream::yielder::pair();
                            ::async_stream::AsyncStream::new(__yield_rx, async move {
                                let sql = if query.selectors.iter().any(Selector::is_empty) {
                                    SelectQuery::<Postgres> {
                                        table_name: query.table_name,
                                        col_names: query.col_names,
                                        selectors: <[_]>::into_vec(box [Selector::new()]),
                                        order_by: query.order_by,
                                        offset: query.offset,
                                        limit: query.limit,
                                    }
                                    .to_string()
                                } else {
                                    query.to_string()
                                };
                                let sqlx_query = create_sqlx_query(
                                    &sql,
                                    query.selectors,
                                    ::alloc::vec::Vec::new(),
                                );
                                {
                                    let mut __pinned = self.fetch(sqlx_query);
                                    let mut __pinned =
                                        unsafe { ::core::pin::Pin::new_unchecked(&mut __pinned) };
                                    loop {
                                        let result =
                                            match ::async_stream::reexport::next(&mut __pinned)
                                                .await
                                            {
                                                ::core::option::Option::Some(e) => e,
                                                ::core::option::Option::None => break,
                                            };
                                        {
                                            let row = match result {
                                                ::core::result::Result::Ok(v) => v,
                                                ::core::result::Result::Err(e) => {
                                                    __yield_tx
                                                        .send(::core::result::Result::Err(e.into()))
                                                        .await;
                                                    return;
                                                }
                                            };
                                            let record = Record::from_row(row);
                                            __yield_tx
                                                .send(::core::result::Result::Ok(record))
                                                .await
                                        }
                                    }
                                }
                            })
                        })
                    }
                }
                fn count<'q, 'o>(
                    self,
                    query: crate::manager::CountQuery<'q, Postgres>,
                ) -> futures::future::BoxFuture<'o, sqlx::Result<u32>>
                where
                    'm: 'o,
                    'q: 'o,
                    for<'a> u32: Type<Postgres> + Decode<'a, Postgres>,
                    for<'a> &'a str: sqlx::ColumnIndex<<Postgres as sqlx::Database>::Row>,
                {
                    if query.selectors.is_empty() {
                        Box::pin(async { Ok(0) })
                    } else {
                        Box::pin(async {
                            let sql = if query.selectors.iter().any(Selector::is_empty) {
                                CountQuery::<Postgres> {
                                    table_name: query.table_name,
                                    selectors: <[_]>::into_vec(box [Selector::new()]),
                                }
                                .to_string()
                            } else {
                                query.to_string()
                            };
                            let sqlx_query =
                                create_sqlx_query(&sql, query.selectors, ::alloc::vec::Vec::new());
                            let row = self.fetch_one(sqlx_query).await?;
                            let count = row.try_get("cnt")?;
                            Ok(count)
                        })
                    }
                }
                fn insert<'q, 'o>(
                    self,
                    query: crate::manager::InsertQuery<'q, Postgres>,
                ) -> futures::future::BoxFuture<'o, sqlx::Result<()>>
                where
                    'm: 'o,
                    'q: 'o,
                {
                    if query.col_names.is_empty() || query.values.is_empty() {
                        Box::pin(async { Ok(()) })
                    } else {
                        Box::pin(async {
                            let sql = query.to_string();
                            let sqlx_query =
                                create_sqlx_query(&sql, ::alloc::vec::Vec::new(), query.values);
                            self.execute(sqlx_query).await?;
                            Ok(())
                        })
                    }
                }
                fn insert_returning<'q, 'o>(
                    self,
                    query: crate::manager::InsertReturningQuery<'q, Postgres>,
                ) -> futures::stream::BoxStream<'o, sqlx::Result<crate::manager::Record<Postgres>>>
                where
                    'm: 'o,
                    'q: 'o,
                {
                    if query.insert_query.col_names.is_empty()
                        || query.insert_query.values.is_empty()
                    {
                        Box::pin(futures::stream::empty())
                    } else {
                        Box::pin({
                            let (mut __yield_tx, __yield_rx) = ::async_stream::yielder::pair();
                            ::async_stream::AsyncStream::new(__yield_rx, async move {
                                let sql = query.to_string();
                                let sqlx_query = create_sqlx_query(
                                    &sql,
                                    ::alloc::vec::Vec::new(),
                                    query.insert_query.values,
                                );
                                {
                                    let mut __pinned = self.fetch(sqlx_query);
                                    let mut __pinned =
                                        unsafe { ::core::pin::Pin::new_unchecked(&mut __pinned) };
                                    loop {
                                        let result =
                                            match ::async_stream::reexport::next(&mut __pinned)
                                                .await
                                            {
                                                ::core::option::Option::Some(e) => e,
                                                ::core::option::Option::None => break,
                                            };
                                        {
                                            let row = match result {
                                                ::core::result::Result::Ok(v) => v,
                                                ::core::result::Result::Err(e) => {
                                                    __yield_tx
                                                        .send(::core::result::Result::Err(e.into()))
                                                        .await;
                                                    return;
                                                }
                                            };
                                            let record = Record::from_row(row);
                                            __yield_tx
                                                .send(::core::result::Result::Ok(record))
                                                .await
                                        }
                                    }
                                }
                            })
                        })
                    }
                }
                fn update<'q, 'o>(
                    self,
                    query: crate::manager::UpdateQuery<'q, Postgres>,
                ) -> futures::future::BoxFuture<'o, sqlx::Result<()>>
                where
                    'm: 'o,
                    'q: 'o,
                {
                    if query.new_values.is_empty() {
                        Box::pin(async { Ok(()) })
                    } else {
                        Box::pin(async {
                            let sql = query.to_string();
                            let sqlx_query = create_sqlx_query(
                                &sql,
                                query.selectors,
                                <[_]>::into_vec(box [query.new_values]),
                            );
                            self.execute(sqlx_query).await?;
                            Ok(())
                        })
                    }
                }
                fn delete<'q, 'o>(
                    self,
                    query: crate::manager::DeleteQuery<'q, Postgres>,
                ) -> futures::future::BoxFuture<'o, sqlx::Result<()>>
                where
                    'm: 'o,
                    'q: 'o,
                {
                    if query.selectors.is_empty() {
                        Box::pin(async { Ok(()) })
                    } else {
                        Box::pin(async {
                            let sql = if query.selectors.iter().any(Selector::is_empty) {
                                DeleteQuery::<Postgres> {
                                    table_name: query.table_name,
                                    selectors: <[_]>::into_vec(box [Selector::new()]),
                                }
                                .to_string()
                            } else {
                                query.to_string()
                            };
                            let sqlx_query =
                                create_sqlx_query(&sql, query.selectors, ::alloc::vec::Vec::new());
                            self.execute(sqlx_query).await?;
                            Ok(())
                        })
                    }
                }
            }
            fn create_sqlx_query<'q, DB: Database>(
                sql: &'q str,
                selectors: Vec<Selector<'q, DB>>,
                input_records: Vec<InputRecord<'q, DB>>,
            ) -> sqlx::query::Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                let mut sqlx_query = sqlx::query(sql);
                for selector in selectors {
                    for (_, op) in selector.into_cols() {
                        match op {
                            FindOperator::Eq(val) | FindOperator::Ne(val) => {
                                sqlx_query = val.bind(sqlx_query)
                            }
                            FindOperator::In(vals) | FindOperator::NotIn(vals) => {
                                for val in vals {
                                    sqlx_query = val.bind(sqlx_query);
                                }
                            }
                        }
                    }
                }
                for input_record in input_records {
                    for (_, val) in input_record.into_cols() {
                        sqlx_query = val.bind(sqlx_query);
                    }
                }
                sqlx_query
            }
        }
    }
    mod value {
        use sqlx::{database::HasArguments, query::Query, Database, Encode, Type};
        use std::any::Any;
        pub trait Value<'q, DB: Database>: Send {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments>;
            fn is_null(&self) -> bool;
            fn to_owned_any(&self) -> Box<dyn Any>;
        }
        impl<'q, DB: Database, T> Value<'q, DB> for &'q T
        where
            T: Value<'q, DB> + Type<DB> + for<'e> Encode<'e, DB> + Sync,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                T::is_null(*self)
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                T::to_owned_any(self)
            }
        }
        impl<'q> Value<'q, sqlx::MySql> for &'q [u8] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::MySql, <sqlx::MySql as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::MySql, <sqlx::MySql as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [u8] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Sqlite> for &'q [u8] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Sqlite, <sqlx::Sqlite as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Sqlite, <sqlx::Sqlite as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        #[cfg(feature = "postgres")]
        impl<'q, 'o> Value<'q, sqlx::Postgres> for &'o [&'q [u8]]
        where
            'o: 'q,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.iter().map(|bytes| bytes.to_vec()).collect::<Vec<_>>())
            }
        }
        #[cfg(feature = "postgres")]
        impl<'q> Value<'q, sqlx::Postgres> for Vec<&'q [u8]> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.iter().map(|bytes| bytes.to_vec()).collect::<Vec<_>>())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for &'q str
        where
            for<'e> &'e str: Type<DB> + Encode<'e, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new((*self).to_owned())
            }
        }
        #[cfg(feature = "postgres")]
        impl<'q, 'o> Value<'q, sqlx::Postgres> for &'o [&'q str]
        where
            'o: 'q,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.iter().map(|str| (*str).to_owned()).collect::<Vec<_>>())
            }
        }
        #[cfg(feature = "postgres")]
        impl<'q> Value<'q, sqlx::Postgres> for Vec<&'q str> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.iter().map(|str| (*str).to_owned()).collect::<Vec<_>>())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for std::borrow::Cow<'q, str>
        where
            for<'e> std::borrow::Cow<'e, str>: Type<DB> + Encode<'e, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone().into_owned())
            }
        }
        #[cfg(feature = "postgres")]
        impl<'q, 'o> Value<'q, sqlx::Postgres> for &'o [std::borrow::Cow<'q, str>]
        where
            'o: 'q,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(
                    self.iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>(),
                )
            }
        }
        #[cfg(feature = "postgres")]
        impl<'q> Value<'q, sqlx::Postgres> for Vec<std::borrow::Cow<'q, str>> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(
                    self.iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>(),
                )
            }
        }
        impl<'q, DB: Database, T> Value<'q, DB> for Option<T>
        where
            T: Value<'q, DB> + Send + 'q,
            Option<T>: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                self.is_none()
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.as_ref().map(|val| T::to_owned_any(val)))
            }
        }
        impl<'q, T> Value<'q, sqlx::Postgres> for &'q [Option<T>]
        where
            Option<T>: Type<sqlx::Postgres> + for<'e> Encode<'e, sqlx::Postgres>,
            T: Send + Sync + Clone + 'static + sqlx::postgres::PgHasArrayType,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q, T> Value<'q, sqlx::Postgres> for Vec<Option<T>>
        where
            Option<T>: Type<sqlx::Postgres> + for<'e> Encode<'e, sqlx::Postgres>,
            T: Send + Sync + Clone + 'static + sqlx::postgres::PgHasArrayType,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for bool
        where
            bool: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for u8
        where
            u8: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for u16
        where
            u16: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for u32
        where
            u32: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for u64
        where
            u64: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for i8
        where
            i8: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for i16
        where
            i16: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for i32
        where
            i32: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for i64
        where
            i64: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for f32
        where
            f32: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for f64
        where
            f64: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for String
        where
            String: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for std::time::Duration
        where
            std::time::Duration: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for Vec<u8>
        where
            Vec<u8>: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [bool] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<bool> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [u32] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<u32> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [i8] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<i8> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [i16] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<i16> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [i32] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<i32> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [i64] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<i64> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [f32] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<f32> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [f64] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<f64> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [String] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<String> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [std::time::Duration] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<std::time::Duration> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [Vec<u8>] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<Vec<u8>> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::BigDecimal
        where
            sqlx::types::BigDecimal: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::BigDecimal] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::BigDecimal> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::Decimal
        where
            sqlx::types::Decimal: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::Decimal] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::Decimal> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for serde_json::Value
        where
            serde_json::Value: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database, T> Value<'q, DB> for sqlx::types::Json<T>
        where
            T: serde::Serialize,
            sqlx::types::Json<T>: Type<DB> + for<'e> Encode<'e, DB> + Send + 'static + Clone,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, T> Value<'q, sqlx::Postgres> for &'q [sqlx::types::Json<T>]
        where
            T: serde::Serialize,
            sqlx::types::Json<T>: Type<sqlx::Postgres>
                + for<'e> Encode<'e, sqlx::Postgres>
                + Send
                + Sync
                + 'static
                + Clone,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q, T> Value<'q, sqlx::Postgres> for Vec<sqlx::types::Json<T>>
        where
            T: serde::Serialize,
            sqlx::types::Json<T>: Type<sqlx::Postgres>
                + for<'e> Encode<'e, sqlx::Postgres>
                + Send
                + Sync
                + 'static
                + Clone,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::time::OffsetDateTime
        where
            sqlx::types::time::OffsetDateTime: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::time::PrimitiveDateTime
        where
            sqlx::types::time::PrimitiveDateTime: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::time::Time
        where
            sqlx::types::time::Time: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::time::UtcOffset
        where
            sqlx::types::time::UtcOffset: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for time_rs::Duration
        where
            time_rs::Duration: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::time::OffsetDateTime] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::time::OffsetDateTime> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::time::PrimitiveDateTime] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::time::PrimitiveDateTime> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::time::Time] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::time::Time> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::chrono::FixedOffset
        where
            sqlx::types::chrono::FixedOffset: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::chrono::Local
        where
            sqlx::types::chrono::Local: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::chrono::NaiveDate
        where
            sqlx::types::chrono::NaiveDate: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::chrono::NaiveTime
        where
            sqlx::types::chrono::NaiveTime: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::chrono::Utc
        where
            sqlx::types::chrono::Utc: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for chrono_rs::Duration
        where
            chrono_rs::Duration: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::chrono::NaiveDate] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::chrono::NaiveDate> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::chrono::NaiveTime] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::chrono::NaiveTime> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [chrono_rs::Duration] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<chrono_rs::Duration> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database, Tz> Value<'q, DB> for sqlx::types::chrono::DateTime<Tz>
        where
            Tz: sqlx::types::chrono::TimeZone,
            sqlx::types::chrono::DateTime<Tz>: Type<DB> + for<'e> Encode<'e, DB> + Send + 'static,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, Tz> Value<'q, sqlx::Postgres> for &'q [sqlx::types::chrono::DateTime<Tz>]
        where
            Tz: sqlx::types::chrono::TimeZone,
            <Tz as sqlx::types::chrono::TimeZone>::Offset: Sync,
            sqlx::types::chrono::DateTime<Tz>:
                Type<sqlx::Postgres> + for<'e> Encode<'e, sqlx::Postgres> + Send + 'static,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q, Tz> Value<'q, sqlx::Postgres> for Vec<sqlx::types::chrono::DateTime<Tz>>
        where
            Tz: sqlx::types::chrono::TimeZone,
            <Tz as sqlx::types::chrono::TimeZone>::Offset: Sync,
            sqlx::types::chrono::DateTime<Tz>:
                Type<sqlx::Postgres> + for<'e> Encode<'e, sqlx::Postgres> + Send + 'static,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::ipnetwork::IpNetwork
        where
            sqlx::types::ipnetwork::IpNetwork: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::ipnetwork::IpNetwork] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::ipnetwork::IpNetwork> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::mac_address::MacAddress
        where
            sqlx::types::mac_address::MacAddress: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::mac_address::MacAddress] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::mac_address::MacAddress> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::uuid::Uuid
        where
            sqlx::types::uuid::Uuid: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::uuid::adapter::Hyphenated
        where
            sqlx::types::uuid::adapter::Hyphenated: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::uuid::Uuid] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::uuid::Uuid> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::BitVec<u32>
        where
            sqlx::types::BitVec<u32>: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::types::BitVec<u32>] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::types::BitVec<u32>> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        #[cfg(feature = "bstr")]
        impl<'q, DB: Database> Value<'q, DB> for &'q sqlx::types::bstr::BStr
        where
            for<'e> &'e sqlx::types::bstr::BStr: Type<DB> + Encode<'e, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new((*self).to_owned())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::bstr::BString
        where
            sqlx::types::bstr::BString: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::types::git2::Oid
        where
            sqlx::types::git2::Oid: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::postgres::types::PgInterval
        where
            sqlx::postgres::types::PgInterval: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::postgres::types::PgLQuery
        where
            sqlx::postgres::types::PgLQuery: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::postgres::types::PgLTree
        where
            sqlx::postgres::types::PgLTree: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB> for sqlx::postgres::types::PgMoney
        where
            sqlx::postgres::types::PgMoney: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::postgres::types::PgInterval] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::postgres::types::PgInterval> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::postgres::types::PgLTree] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::postgres::types::PgLTree> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for &'q [sqlx::postgres::types::PgMoney] {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.to_vec())
            }
        }
        impl<'q> Value<'q, sqlx::Postgres> for Vec<sqlx::postgres::types::PgMoney> {
            fn bind(
                self: Box<Self>,
                query: Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, sqlx::Postgres, <sqlx::Postgres as HasArguments<'q>>::Arguments>
            {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB>
            for sqlx::postgres::types::PgTimeTz<
                sqlx::types::chrono::NaiveTime,
                sqlx::types::chrono::FixedOffset,
            >
        where
            sqlx::postgres::types::PgTimeTz<
                sqlx::types::chrono::NaiveTime,
                sqlx::types::chrono::FixedOffset,
            >: Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
        impl<'q, DB: Database> Value<'q, DB>
            for sqlx::postgres::types::PgTimeTz<
                sqlx::types::time::Time,
                sqlx::types::time::UtcOffset,
            >
        where
            sqlx::postgres::types::PgTimeTz<sqlx::types::time::Time, sqlx::types::time::UtcOffset>:
                Type<DB> + Encode<'q, DB>,
        {
            fn bind(
                self: Box<Self>,
                query: Query<'q, DB, <DB as HasArguments<'q>>::Arguments>,
            ) -> Query<'q, DB, <DB as HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }
            fn is_null(&self) -> bool {
                false
            }
            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> Box<dyn Any> {
                Box::new(self.clone())
            }
        }
    }
    pub use value::Value;
    #[cfg(all(feature = "test-manager", feature = "sqlite"))]
    pub trait Manager<'m, DB: Database>: Send {
        type Error: Error + Send + Sync + 'static;
        fn select<'q, 'o>(
            self,
            query: SelectQuery<'q, DB>,
        ) -> BoxStream<'o, Result<Record<DB>, Self::Error>>
        where
            'm: 'o,
            'q: 'o;
        fn count<'q, 'o>(
            self,
            query: CountQuery<'q, DB>,
        ) -> BoxFuture<'o, Result<u32, Self::Error>>
        where
            'm: 'o,
            'q: 'o,
            for<'a> u32: Type<DB> + Decode<'a, DB>,
            for<'a> &'a str: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>;
        fn insert<'q, 'o>(
            self,
            query: InsertQuery<'q, DB>,
        ) -> BoxFuture<'o, Result<(), Self::Error>>
        where
            'm: 'o,
            'q: 'o;
        fn insert_returning<'q, 'o>(
            self,
            query: InsertReturningQuery<'q, DB>,
        ) -> BoxStream<'o, Result<Record<DB>, Self::Error>>
        where
            'm: 'o,
            'q: 'o;
        fn update<'q, 'o>(
            self,
            query: UpdateQuery<'q, DB>,
        ) -> BoxFuture<'o, Result<(), Self::Error>>
        where
            'm: 'o,
            'q: 'o;
        fn delete<'q, 'o>(
            self,
            query: DeleteQuery<'q, DB>,
        ) -> BoxFuture<'o, Result<(), Self::Error>>
        where
            'm: 'o,
            'q: 'o;
    }
    pub struct SelectQuery<'q, DB: Database> {
        pub table_name: &'q str,
        pub col_names: &'q [&'q str],
        pub selectors: Vec<Selector<'q, DB>>,
        pub order_by: Option<OrderBy<&'q str>>,
        pub offset: Option<u32>,
        pub limit: Option<u32>,
    }
    pub struct CountQuery<'q, DB: Database> {
        pub table_name: &'q str,
        pub selectors: Vec<Selector<'q, DB>>,
    }
    pub struct InsertQuery<'q, DB: Database> {
        pub table_name: &'q str,
        pub col_names: &'q [&'q str],
        pub values: Vec<InputRecord<'q, DB>>,
    }
    pub struct InsertReturningQuery<'q, DB: Database> {
        pub insert_query: InsertQuery<'q, DB>,
        pub returning_cols: &'q [&'q str],
    }
    pub struct UpdateQuery<'q, DB: Database> {
        pub table_name: &'q str,
        pub selectors: Vec<Selector<'q, DB>>,
        pub new_values: InputRecord<'q, DB>,
    }
    pub struct DeleteQuery<'q, DB: Database> {
        pub table_name: &'q str,
        pub selectors: Vec<Selector<'q, DB>>,
    }
    pub struct Selector<'q, DB: Database>(
        BTreeMap<&'q str, FindOperator<Box<dyn Value<'q, DB> + 'q>>>,
    );
    impl<'q, DB: Database> Selector<'q, DB> {
        pub fn new() -> Self {
            Self(BTreeMap::new())
        }
        pub fn len(&self) -> usize {
            self.0.len()
        }
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        pub fn add_col(
            &mut self,
            col_name: &'q str,
            operator: FindOperator<Box<dyn Value<'q, DB> + 'q>>,
        ) {
            self.0.insert(col_name, operator);
        }
        pub fn remove_col(&mut self, col_name: &str) {
            self.0.remove(col_name);
        }
        pub fn has_col(&self, col_name: &str) -> bool {
            self.0.contains_key(col_name)
        }
        pub fn col(&self, col_name: &str) -> Option<&FindOperator<Box<dyn Value<'q, DB> + 'q>>> {
            self.0.get(col_name)
        }
        pub fn cols(
            &self,
        ) -> impl ExactSizeIterator<Item = (&'q str, &FindOperator<Box<dyn Value<'q, DB> + 'q>>)>
        {
            self.0
                .iter()
                .map(|(col_name, operator)| (*col_name, operator))
        }
        pub fn into_cols(
            self,
        ) -> impl ExactSizeIterator<Item = (&'q str, FindOperator<Box<dyn Value<'q, DB> + 'q>>)>
        {
            self.0.into_iter()
        }
    }
    impl<'q, DB: Database> Default for Selector<'q, DB> {
        fn default() -> Self {
            Self::new()
        }
    }
    pub trait IntoSelector<'q, DB: Database> {
        fn into_selector(self) -> Selector<'q, DB>;
    }
    pub enum FindOperator<T> {
        Eq(T),
        Ne(T),
        In(Vec<T>),
        NotIn(Vec<T>),
    }
    impl<T> FindOperator<T> {
        pub fn boxed<'q, DB: Database>(self) -> FindOperator<Box<dyn Value<'q, DB> + 'q>>
        where
            T: Value<'q, DB> + 'q,
        {
            match self {
                Self::Eq(val) => FindOperator::Eq(Box::new(val)),
                Self::Ne(val) => FindOperator::Ne(Box::new(val)),
                Self::In(vals) => FindOperator::In(
                    vals.into_iter()
                        .map(|val| Box::new(val) as Box<dyn Value<'q, _>>)
                        .collect(),
                ),
                Self::NotIn(vals) => FindOperator::NotIn(
                    vals.into_iter()
                        .map(|val| Box::new(val) as Box<dyn Value<'q, _>>)
                        .collect(),
                ),
            }
        }
    }
    pub struct InputRecord<'q, DB: Database>(BTreeMap<&'q str, Box<dyn Value<'q, DB>>>);
    impl<'q, DB: Database> InputRecord<'q, DB> {
        pub fn new() -> Self {
            Self(BTreeMap::new())
        }
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        pub fn has_col(&self, col_name: &str) -> bool {
            self.0.contains_key(col_name)
        }
        pub fn add_col(&mut self, col_name: &'q str, value: Box<dyn Value<'q, DB>>) {
            self.0.insert(col_name, value);
        }
        pub fn remove_col(&mut self, col_name: &str) {
            self.0.remove(col_name);
        }
        pub fn col(&self, col_name: &str) -> Option<&Box<dyn Value<'q, DB>>> {
            self.0.get(col_name)
        }
        pub fn cols(&self) -> impl ExactSizeIterator<Item = (&'q str, &Box<dyn Value<'q, DB>>)> {
            self.0.iter().map(|(col_name, value)| (*col_name, value))
        }
        pub fn into_cols(self) -> impl ExactSizeIterator<Item = (&'q str, Box<dyn Value<'q, DB>>)> {
            self.0
                .into_iter()
                .map(|(col_name, value)| (col_name, value))
        }
    }
    impl<'q, DB: Database> Default for InputRecord<'q, DB> {
        fn default() -> Self {
            Self::new()
        }
    }
    pub trait IntoInputRecord<'q, DB: Database> {
        fn to_input_record(&self) -> InputRecord<'q, DB>;
        fn into_input_record(self) -> InputRecord<'q, DB>;
    }
    pub struct OrderBy<C> {
        pub order: Order,
        pub cols: Vec<C>,
    }
    pub enum Order {
        Asc,
        Desc,
    }
    pub struct Record<DB: Database> {
        row: Option<DB::Row>,
        map: BTreeMap<String, Box<dyn Any + Send>>,
    }
    impl<DB: Database> Record<DB> {
        pub fn new() -> Self {
            Self {
                row: None,
                map: BTreeMap::new(),
            }
        }
        pub fn from_row(row: DB::Row) -> Self {
            Self {
                row: Some(row),
                map: BTreeMap::new(),
            }
        }
        pub fn col<T: sqlx::Type<DB> + for<'d> sqlx::Decode<'d, DB> + Clone + Any>(
            &self,
            col_name: &str,
        ) -> Result<T, RecordError>
        where
            for<'a> &'a str: sqlx::ColumnIndex<DB::Row>,
        {
            if let Some(entry) = self.map.get(col_name) {
                entry.as_ref().downcast_ref::<T>().cloned().ok_or_else(|| {
                    RecordError::ColumnDecode {
                        index: col_name.into(),
                        source: None,
                    }
                })
            } else if let Some(row) = self.row.as_ref() {
                row.try_get::<T, _>(col_name).map_err(|e| match e {
                    sqlx::Error::ColumnNotFound(c) => RecordError::ColumnNotFound(c),
                    sqlx::Error::ColumnDecode { index, source } => RecordError::ColumnDecode {
                        index,
                        source: Some(source),
                    },
                    _ => ::core::panicking::panic("internal error: entered unreachable code"),
                })
            } else {
                Err(RecordError::ColumnNotFound(col_name.into()))
            }
        }
    }
    impl<DB: Database> Default for Record<DB> {
        fn default() -> Self {
            Self::new()
        }
    }
    pub enum RecordError {
        #[error("column not found: {0}")]
        ColumnNotFound(String),
        #[error("error decoding column {index}: {source:?}")]
        ColumnDecode {
            index: String,
            source: Option<Box<dyn std::error::Error + Send + Sync>>,
        },
    }
    #[automatically_derived]
    #[allow(unused_qualifications)]
    impl ::core::fmt::Debug for RecordError {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            match (&*self,) {
                (&RecordError::ColumnNotFound(ref __self_0),) => {
                    let debug_trait_builder =
                        &mut ::core::fmt::Formatter::debug_tuple(f, "ColumnNotFound");
                    let _ = ::core::fmt::DebugTuple::field(debug_trait_builder, &&(*__self_0));
                    ::core::fmt::DebugTuple::finish(debug_trait_builder)
                }
                (&RecordError::ColumnDecode {
                    index: ref __self_0,
                    source: ref __self_1,
                },) => {
                    let debug_trait_builder =
                        &mut ::core::fmt::Formatter::debug_struct(f, "ColumnDecode");
                    let _ = ::core::fmt::DebugStruct::field(
                        debug_trait_builder,
                        "index",
                        &&(*__self_0),
                    );
                    let _ = ::core::fmt::DebugStruct::field(
                        debug_trait_builder,
                        "source",
                        &&(*__self_1),
                    );
                    ::core::fmt::DebugStruct::finish(debug_trait_builder)
                }
            }
        }
    }
    #[allow(unused_qualifications)]
    impl std::error::Error for RecordError {
        fn source(&self) -> std::option::Option<&(dyn std::error::Error + 'static)> {
            use thiserror::private::AsDynError;
            #[allow(deprecated)]
            match self {
                RecordError::ColumnNotFound { .. } => std::option::Option::None,
                RecordError::ColumnDecode { source: source, .. } => {
                    std::option::Option::Some(source.as_ref()?.as_dyn_error())
                }
            }
        }
    }
    #[allow(unused_qualifications)]
    impl std::fmt::Display for RecordError {
        fn fmt(&self, __formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            #[allow(unused_imports)]
            use thiserror::private::{DisplayAsDisplay, PathAsDisplay};
            #[allow(unused_variables, deprecated, clippy::used_underscore_binding)]
            match self {
                RecordError::ColumnNotFound(_0) => {
                    __formatter.write_fmt(::core::fmt::Arguments::new_v1(
                        &["column not found: "],
                        &match (&_0.as_display(),) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    ))
                }
                RecordError::ColumnDecode { index, source } => {
                    __formatter.write_fmt(::core::fmt::Arguments::new_v1(
                        &["error decoding column ", ": "],
                        &match (&index.as_display(), &source) {
                            (arg0, arg1) => [
                                ::core::fmt::ArgumentV1::new(arg0, ::core::fmt::Display::fmt),
                                ::core::fmt::ArgumentV1::new(arg1, ::core::fmt::Debug::fmt),
                            ],
                        },
                    ))
                }
            }
        }
    }
    pub trait FromRecord<DB: Database>: Sized {
        fn from_record(record: &Record<DB>) -> Result<Self, RecordError>;
    }
}
