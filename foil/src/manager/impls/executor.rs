use crate::{
    manager::{CountQuery, DeleteQuery, FindOperator, InputRecord, Record, SelectQuery, Selector},
    Manager,
};
use futures::stream::BoxStream;
use sqlx::{database::HasArguments, Database, Decode, Executor, Row, Type};

macro_rules! impl_manager_for_db_executor {
    ($DB:path) => {
        impl<'m, T> Manager<'m, $DB> for T
        where
            T: Executor<'m, Database = $DB> + 'm,
        {
            type Error = sqlx::Error;

            fn select<'q, 'o>(
                self,
                query: crate::manager::SelectQuery<'q, $DB>,
            ) -> futures::stream::BoxStream<'o, sqlx::Result<crate::manager::Record<$DB>>>
            where
                'm: 'o,
                'q: 'o,
            {
                if query.col_names.is_empty() {
                    Box::pin(futures::stream::once(async { Ok(Record::new()) }))
                } else if query.selectors.is_empty()
                    || query.selectors.iter().any(|selector| {
                        selector.cols().any(|(_, find_operator)| {
                            if let FindOperator::In(vals) = find_operator {
                                vals.is_empty()
                            } else {
                                false
                            }
                        })
                    })
                {
                    Box::pin(futures::stream::empty())
                } else {
                    Box::pin(async_stream::try_stream! {
                        let sql = if query.selectors.iter().any(Selector::is_empty) {
                            SelectQuery::<$DB> {
                                table_name: query.table_name,
                                col_names: query.col_names,
                                selectors: vec![Selector::new()],
                                order_by: query.order_by,
                                offset: query.offset,
                                limit: query.limit
                            }.to_string()
                        } else {
                            query.to_string()
                        };

                        let sqlx_query = create_sqlx_query(&sql, query.selectors, vec![]);

                        for await result in self.fetch(sqlx_query) {
                            let row = result?;
                            let record = Record::from_row(row);
                            yield record
                        }
                    })
                }
            }

            fn count<'q, 'o>(
                self,
                query: crate::manager::CountQuery<'q, $DB>,
            ) -> futures::future::BoxFuture<'o, sqlx::Result<u32>>
            where
                'm: 'o,
                'q: 'o,
                for<'a> u32: Type<$DB> + Decode<'a, $DB>,
                for<'a> &'a str: sqlx::ColumnIndex<<$DB as sqlx::Database>::Row>,
            {
                if query.selectors.is_empty()
                    || query.selectors.iter().any(|selector| {
                        selector.cols().any(|(_, find_operator)| {
                            if let FindOperator::In(vals) = find_operator {
                                vals.is_empty()
                            } else {
                                false
                            }
                        })
                    })
                {
                    Box::pin(async { Ok(0) })
                } else {
                    Box::pin(async {
                        let sql = if query.selectors.iter().any(Selector::is_empty) {
                            CountQuery::<$DB> {
                                table_name: query.table_name,
                                selectors: vec![Selector::new()],
                            }
                            .to_string()
                        } else {
                            query.to_string()
                        };

                        let sqlx_query = create_sqlx_query(&sql, query.selectors, vec![]);

                        let row = self.fetch_one(sqlx_query).await?;

                        let count = row.try_get("cnt")?;

                        Ok(count)
                    })
                }
            }

            fn insert<'q, 'o>(
                self,
                query: crate::manager::InsertQuery<'q, $DB>,
            ) -> futures::future::BoxFuture<'o, sqlx::Result<()>>
            where
                'm: 'o,
                'q: 'o,
            {
                if query.values.is_empty()
                    || query
                        .values
                        .iter()
                        .all(|input_record| input_record.is_empty())
                {
                    Box::pin(async { Ok(()) })
                } else {
                    Box::pin(async {
                        let sql = query.to_string();
                        let sqlx_query = create_sqlx_query(&sql, vec![], query.values);

                        self.execute(sqlx_query).await?;

                        Ok(())
                    })
                }
            }

            fn insert_returning<'q, 'o>(
                self,
                query: crate::manager::InsertReturningQuery<'q, $DB>,
            ) -> futures::stream::BoxStream<'o, sqlx::Result<crate::manager::Record<$DB>>>
            where
                'm: 'o,
                'q: 'o,
            {
                if query.insert_query.values.is_empty()
                    || query
                        .insert_query
                        .values
                        .iter()
                        .all(|input_record| input_record.is_empty())
                {
                    Box::pin(futures::stream::empty())
                } else {
                    Box::pin(async_stream::try_stream! {
                        let sql = query.to_string();
                        let sqlx_query = create_sqlx_query(&sql, vec![], query.insert_query.values);

                        for await result in self.fetch(sqlx_query) {
                            let row = result?;
                            let record = Record::from_row(row);
                            yield record
                        }
                    })
                }
            }

            fn update<'q, 'o>(
                self,
                query: crate::manager::UpdateQuery<'q, $DB>,
            ) -> futures::future::BoxFuture<'o, sqlx::Result<()>>
            where
                'm: 'o,
                'q: 'o,
            {
                if query.selectors.is_empty()
                    || query.selectors.iter().any(|selector| {
                        selector.cols().any(|(_, find_operator)| {
                            if let FindOperator::In(vals) = find_operator {
                                vals.is_empty()
                            } else {
                                false
                            }
                        })
                    })
                    || query.new_values.is_empty()
                {
                    Box::pin(async { Ok(()) })
                } else {
                    Box::pin(async {
                        let sql = query.to_string();
                        let sqlx_query =
                            create_sqlx_query(&sql, query.selectors, vec![query.new_values]);

                        self.execute(sqlx_query).await?;

                        Ok(())
                    })
                }
            }

            fn delete<'q, 'o>(
                self,
                query: crate::manager::DeleteQuery<'q, $DB>,
            ) -> futures::future::BoxFuture<'o, sqlx::Result<()>>
            where
                'm: 'o,
                'q: 'o,
            {
                if query.selectors.is_empty()
                    || query.selectors.iter().any(|selector| {
                        selector.cols().any(|(_, find_operator)| {
                            if let FindOperator::In(vals) = find_operator {
                                vals.is_empty()
                            } else {
                                false
                            }
                        })
                    })
                {
                    Box::pin(async { Ok(()) })
                } else {
                    Box::pin(async {
                        let sql = if query.selectors.iter().any(Selector::is_empty) {
                            DeleteQuery::<$DB> {
                                table_name: query.table_name,
                                selectors: vec![Selector::new()],
                            }
                            .to_string()
                        } else {
                            query.to_string()
                        };

                        let sqlx_query = create_sqlx_query(&sql, query.selectors, vec![]);

                        self.execute(sqlx_query).await?;

                        Ok(())
                    })
                }
            }

            fn query<'q, 'o, Q: sqlx::Execute<'q, $DB> + 'q>(
                self,
                query: Q,
            ) -> BoxStream<'o, Result<Record<$DB>, Self::Error>>
            where
                'm: 'o,
                'q: 'o,
            {
                Box::pin(async_stream::try_stream! {
                    for await result in self.fetch(query) {
                        let row = result?;
                        let record = Record::from_row(row);
                        yield record
                    }
                })
            }
        }
    };
}

#[cfg(feature = "mysql")]
impl_manager_for_db_executor!(sqlx::MySql);

#[cfg(feature = "mssql")]
impl_manager_for_db_executor!(sqlx::Mssql);

#[cfg(feature = "postgres")]
impl_manager_for_db_executor!(sqlx::Postgres);

#[cfg(feature = "sqlite")]
impl_manager_for_db_executor!(sqlx::Sqlite);

#[cfg(feature = "any")]
impl_manager_for_db_executor!(sqlx::Any);

fn create_sqlx_query<'s, 'q: 's, DB: Database>(
    sql: &'s str,
    selectors: Vec<Selector<'q, DB>>,
    input_records: Vec<InputRecord<'q, DB>>,
) -> sqlx::query::Query<'s, DB, <DB as HasArguments<'s>>::Arguments> {
    // https://github.com/launchbadge/sqlx/issues/1428#issuecomment-1002818746
    let selectors = unsafe { std::mem::transmute::<_, Vec<Selector<'s, DB>>>(selectors) };
    let input_records =
        unsafe { std::mem::transmute::<_, Vec<InputRecord<'s, DB>>>(input_records) };

    let mut sqlx_query = sqlx::query(sql);

    for input_record in input_records {
        for (_, val) in input_record.into_cols() {
            sqlx_query = val.bind(sqlx_query);
        }
    }

    for selector in selectors {
        for (_, op) in selector.into_cols() {
            match op {
                FindOperator::Eq(val) | FindOperator::Ne(val) => sqlx_query = val.bind(sqlx_query),
                FindOperator::In(vals) | FindOperator::NotIn(vals) => {
                    for val in vals {
                        sqlx_query = val.bind(sqlx_query);
                    }
                }
            }
        }
    }

    sqlx_query
}
