use sqlx::{database::HasArguments, Database, Decode, Executor, Row, Type};
use vec1::{vec1, Vec1};

use crate::{
    manager::{Condition, FindOperator, Record, Values},
    Manager,
};

macro_rules! impl_manager_for_db_executor {
    ($DB:path) => {
        impl<'m, T> Manager<'m, $DB> for T
        where
            T: Executor<'m, Database = $DB> + 'm,
        {
            type Error = sqlx::Error;

            fn select<'o, 'q>(
                self,
                query: crate::manager::SelectQuery<'q, $DB>,
            ) -> futures::stream::BoxStream<'o, sqlx::Result<crate::manager::Record<$DB>>>
            where
                'm: 'o,
                'q: 'o,
            {
                Box::pin(async_stream::try_stream! {
                    let sql = query.to_string();
                    let sqlx_query = create_sqlx_query(&sql, query.conds.into(), vec![]);

                    for await result in self.fetch(sqlx_query) {
                        let row = result?;
                        let record = Record::from_row(row);
                        yield record
                    }
                })
            }

            fn count<'o, 'q>(
                self,
                query: crate::manager::CountQuery<'q, $DB>,
            ) -> futures::future::BoxFuture<'o, sqlx::Result<u32>>
            where
                'm: 'o,
                'q: 'o,
                for<'a> u32: Type<$DB> + Decode<'a, $DB>,
                for<'a> &'a str: sqlx::ColumnIndex<<$DB as sqlx::Database>::Row>,
            {
                Box::pin(async {
                    let sql = query.to_string();
                    let sqlx_query = create_sqlx_query(&sql, query.conds.into(), vec![]);

                    let row = self.fetch_one(sqlx_query).await?;

                    let count = row.try_get("cnt")?;

                    Ok(count)
                })
            }

            fn insert<'o, 'q>(
                self,
                query: crate::manager::InsertQuery<'q, $DB>,
            ) -> futures::future::BoxFuture<'o, sqlx::Result<()>>
            where
                'm: 'o,
                'q: 'o,
            {
                Box::pin(async {
                    let sql = query.to_string();
                    let sqlx_query = create_sqlx_query(&sql, vec![], query.values.into());

                    self.execute(sqlx_query).await?;

                    Ok(())
                })
            }

            fn insert_returning<'o, 'q>(
                self,
                query: crate::manager::InsertReturningQuery<'q, $DB>,
            ) -> futures::stream::BoxStream<'o, sqlx::Result<crate::manager::Record<$DB>>>
            where
                'm: 'o,
                'q: 'o,
            {
                Box::pin(async_stream::try_stream! {
                    let sql = query.to_string();
                    let sqlx_query = create_sqlx_query(&sql, vec![], query.insert_query.values.into());

                    for await result in self.fetch(sqlx_query) {
                        let row = result?;
                        let record = Record::from_row(row);
                        yield record
                    }
                })
            }

            fn update<'o, 'q>(
                self,
                query: crate::manager::UpdateQuery<'q, $DB>,
            ) -> futures::future::BoxFuture<'o, sqlx::Result<()>>
            where
                'm: 'o,
                'q: 'o,
            {
                Box::pin(async {
                    let sql = query.to_string();
                    let sqlx_query =
                        create_sqlx_query(&sql, query.conds.into(), vec![query.new_values]);

                    self.execute(sqlx_query).await?;

                    Ok(())
                })
            }

            fn delete<'o, 'q>(
                self,
                query: crate::manager::DeleteQuery<'q, $DB>,
            ) -> futures::future::BoxFuture<'o, sqlx::Result<()>>
            where
                'm: 'o,
                'q: 'o,
            {
                Box::pin(async {
                    let sql = query.to_string();
                    let sqlx_query = create_sqlx_query(&sql, query.conds.into(), vec![]);

                    self.execute(sqlx_query).await?;

                    Ok(())
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

fn create_sqlx_query<'q, DB: Database>(
    sql: &'q str,
    conds: Vec<Condition<DB>>,
    values: Vec<Values<DB>>,
) -> sqlx::query::Query<'q, DB, <DB as HasArguments<'_>>::Arguments> {
    let mut sqlx_query = sqlx::query(sql);

    for cond in conds {
        for (_, op) in cond.into_cols() {
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

    for values in values {
        for (_, val) in values.into_cols() {
            sqlx_query = val.bind(sqlx_query);
        }
    }

    sqlx_query
}
