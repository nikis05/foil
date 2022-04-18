use super::{Col, Field};
use crate::{
    entity::{FindOptions, OrderBy},
    manager::{
        impls::mock::MockManager, FindOperator, FromRecord, InputRecord, IntoSelector, Order,
        Record, RecordError, Selector, ToInputRecord, Value,
    },
    Create, Delete, Entity, Update,
};
use insta::{assert_debug_snapshot, assert_snapshot};
use sqlx::Sqlite;

#[derive(Debug)]
struct Character {
    id: u8,
    name: String,
    is_handsome: bool,
    father_name: Option<String>,
}

impl Entity<Sqlite> for Character {
    type Col = CharacterCol;
    type Id = u8;
    type Selector<'q> = CharacterSelector<'q>;

    fn table_name() -> &'static str {
        "character"
    }

    fn col_names() -> &'static [&'static str] {
        &["id", "name", "is_handsome", "father_name"]
    }

    fn id_col_name() -> &'static str {
        "id"
    }

    fn id(&self) -> Self::Id {
        self.id
    }
}

impl FromRecord<Sqlite> for Character {
    fn from_record(record: &crate::manager::Record<Sqlite>) -> Result<Self, RecordError> {
        Ok(Character {
            id: record.col("id")?,
            name: record.col("name")?,
            is_handsome: record.col("is_handsome")?,
            father_name: record.col("father_name")?,
        })
    }
}

#[derive(Clone, Copy)]
enum CharacterCol {
    Id,
    Name,
    IsHandsome,
    FatherName,
}

impl Col for CharacterCol {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Name => "name",
            Self::IsHandsome => "is_hansome",
            Self::FatherName => "father_name",
        }
    }
}

#[derive(Default)]
struct CharacterSelector<'q> {
    id: Field<FindOperator<u8>>,
    name: Field<FindOperator<&'q str>>,
    is_handsome: Field<FindOperator<bool>>,
    father_name: Field<FindOperator<Option<&'q str>>>,
}

impl<'q> IntoSelector<'q, Sqlite> for CharacterSelector<'q> {
    fn into_selector(self) -> Selector<'q, Sqlite> {
        let mut selector = Selector::new();

        if let Field::Set(op) = self.id {
            selector.add_col("id", op.boxed());
        }

        if let Field::Set(op) = self.name {
            selector.add_col("name", op.boxed());
        }

        if let Field::Set(op) = self.is_handsome {
            selector.add_col("is_handsome", op.boxed());
        }

        if let Field::Set(op) = self.father_name {
            selector.add_col("father_name", op.boxed());
        }

        selector
    }
}

impl Create<Sqlite> for Character {
    type Input<'q> = CharacterInput<'q>;

    fn generated_col_names() -> &'static [&'static str] {
        &["father_name"]
    }

    fn construct<'q>(
        input: &Self::Input<'q>,
        generated: &Record<Sqlite>,
    ) -> Result<Self, RecordError> {
        Ok(Self {
            id: if let Field::Set(id) = input.id {
                id
            } else {
                generated.col("id")?
            },
            name: input.name.to_owned(),
            is_handsome: input.is_handsome,
            father_name: if let Field::Set(father_name) = input.father_name {
                father_name.map(ToOwned::to_owned)
            } else {
                generated.col("father_name")?
            },
        })
    }
}

struct CharacterInput<'q> {
    id: Field<u8>,
    name: &'q str,
    is_handsome: bool,
    father_name: Field<Option<&'q str>>,
}

impl<'q> From<&'q Character> for CharacterInput<'q> {
    fn from(from: &'q Character) -> Self {
        Self {
            id: Field::Set(from.id),
            name: &from.name,
            is_handsome: from.is_handsome,
            father_name: Field::Set(from.father_name.as_deref()),
        }
    }
}

impl<'q> ToInputRecord<'q, Sqlite> for CharacterInput<'q> {
    fn to_input_record(&self) -> InputRecord<'q, Sqlite> {
        let mut values = InputRecord::new();
        if let Field::Set(id) = self.id {
            values.add_col("id", Box::new(id));
        }
        values.add_col("name", Box::new(self.name));
        values.add_col("is_handsome", Box::new(self.is_handsome));
        if let Field::Set(father_name) = self.father_name {
            values.add_col("father_name", Box::new(father_name));
        }
        values
    }
}

impl Update<Sqlite> for Character {
    type Patch<'q> = CharacterPatch<'q>;

    fn apply_patch(&mut self, patch: Self::Patch<'_>) {
        if let Field::Set(id) = patch.id {
            self.id = id;
        }
        if let Field::Set(name) = patch.name {
            self.name = name.to_owned();
        }
        if let Field::Set(is_handsome) = patch.is_handsome {
            self.is_handsome = is_handsome;
        }
        if let Field::Set(father_name) = patch.father_name {
            self.father_name = father_name.map(ToOwned::to_owned);
        }
    }
}

#[derive(Default)]
struct CharacterPatch<'q> {
    id: Field<u8>,
    name: Field<&'q str>,
    is_handsome: Field<bool>,
    father_name: Field<Option<&'q str>>,
}

impl<'q> ToInputRecord<'q, Sqlite> for CharacterPatch<'q> {
    fn to_input_record(&self) -> InputRecord<'q, Sqlite> {
        let mut values = InputRecord::new();
        if let Field::Set(id) = self.id {
            values.add_col("id", Box::new(id));
        }
        if let Field::Set(name) = self.name {
            values.add_col("name", Box::new(name));
        }
        if let Field::Set(is_handsome) = self.is_handsome {
            values.add_col("is_handsome", Box::new(is_handsome));
        }
        if let Field::Set(father_name) = self.father_name {
            values.add_col("father_name", Box::new(father_name));
        }
        values
    }
}

impl Delete<Sqlite> for Character {}

async fn setup() -> MockManager {
    let mut manager = MockManager::new().await.unwrap();
    manager
                .exec_sql(
                    "CREATE TABLE \"character\" (id INTEGER PRIMARY KEY, name TEXT NOT NULL, is_handsome BOOL NOT NULL, father_name TEXT DEFAULT NULL)",
                )
                .await
                .unwrap();

    Character::insert(
        &mut manager,
        vec![
            CharacterInput {
                id: Field::Set(0),
                name: "Legalas",
                is_handsome: true,
                father_name: Field::Set(None),
            },
            CharacterInput {
                id: Field::Set(1),
                name: "Himmly",
                is_handsome: false,
                father_name: Field::Set(Some("Gloyne")),
            },
            CharacterInput {
                id: Field::Set(2),
                name: "Aragorn",
                is_handsome: true,
                father_name: Field::Set(Some("Arathorn")),
            },
        ],
    )
    .await
    .unwrap();

    manager
}

mod entity {
    use super::*;

    mod find {
        use futures::TryStreamExt;

        use super::*;

        #[tokio::test]
        async fn eq() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Set(FindOperator::Eq(true)),
                    father_name: Field::Omit,
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "is_handsome" = ?"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn ne() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Set(FindOperator::Ne(true)),
                    father_name: Field::Omit,
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "is_handsome" != ?"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn in_() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Set(FindOperator::In(vec!["Himmly".into(), "Legalas".into()])),
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "name" IN (?, ?)"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn not_in() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Set(FindOperator::NotIn(vec!["Himmly".into(), "Legalas".into()])),
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "name" NOT IN (?, ?)"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn eq_null() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Set(FindOperator::Eq(None)),
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "father_name" IS NULL"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn ne_null() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Set(FindOperator::Ne(None)),
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "father_name" IS NOT NULL"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn in_with_null() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Set(FindOperator::In(vec![Some("Arathorn".into()), None])),
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "father_name" IN (?) OR "father_name" IS NULL"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn not_in_with_null() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Set(FindOperator::NotIn(vec![
                        Some("Arathorn".into()),
                        None,
                    ])),
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "father_name" NOT IN (?) AND "father_name" IS NOT NULL"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn empty_cond() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character""###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn multiple_fields() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Set(FindOperator::Eq(true)),
                    father_name: Field::Set(FindOperator::Ne(None)),
                }],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE ("is_handsome" = ?) AND ("father_name" IS NOT NULL)"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn multiple_conds() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![
                    CharacterSelector {
                        id: Field::Omit,
                        name: Field::Set(FindOperator::Eq("Legalas".into())),
                        is_handsome: Field::Omit,
                        father_name: Field::Omit,
                    },
                    CharacterSelector {
                        id: Field::Omit,
                        name: Field::Omit,
                        is_handsome: Field::Set(FindOperator::Eq(false)),
                        father_name: Field::Omit,
                    },
                ],
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE ("name" = ?) OR ("is_handsome" = ?)"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn finalizer_optional() {
            let mut manager = setup().await;

            let character = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Set(FindOperator::Eq("Golum".into())),
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .optional()
            .await
            .unwrap();

            assert_debug_snapshot!(character, @"None");

            let character = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Set(FindOperator::Eq("Legalas".into())),
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .optional()
            .await
            .unwrap();

            assert_debug_snapshot!(character, @r###"
            Some(
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
            )
            "###);
        }

        #[tokio::test]
        async fn finalizer_one() {
            let mut manager = setup().await;

            let character = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Set(FindOperator::Eq("Golum".into())),
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .one()
            .await;

            assert_debug_snapshot!(character, @r###"
            Err(
                RowNotFound,
            )
            "###);

            let character = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Set(FindOperator::Eq("Legalas".into())),
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .one()
            .await;

            assert_debug_snapshot!(character, @r###"
            Ok(
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
            )
            "###);
        }

        #[tokio::test]
        async fn finalizer_all() {
            let mut manager = setup().await;

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .all()
            .await
            .unwrap();

            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn finalizer_stream() {
            let mut manager = setup().await;

            let mut characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .stream();

            let ch1 = characters.try_next().await.unwrap();
            assert_debug_snapshot!(ch1, @r###"
            Some(
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
            )
            "###);

            let ch2 = characters.try_next().await.unwrap();
            assert_debug_snapshot!(ch2, @r###"
            Some(
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
            )
            "###);

            let ch3 = characters.try_next().await.unwrap();
            assert_debug_snapshot!(ch3, @r###"
            Some(
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            )
            "###);

            let ch4 = characters.try_next().await.unwrap();
            assert_debug_snapshot!(ch4, @"None");
        }
    }

    mod find_with {
        use super::*;

        #[tokio::test]
        async fn no_options() {
            let mut manager = setup().await;
            let characters = Character::find_with_options(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
                FindOptions {
                    order_by: None,
                    offset: None,
                    limit: None,
                },
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character""###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn order_by() {
            let mut manager = setup().await;
            let characters = Character::find_with_options(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
                FindOptions {
                    order_by: Some(OrderBy {
                        cols: vec![CharacterCol::Id],
                        order: Order::Desc,
                    }),
                    offset: None,
                    limit: None,
                },
            )
            .all()
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" ORDER BY "id" DESC"###);
            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: true,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: true,
                    father_name: None,
                },
            ]
            "###);
        }

        async fn offset() {
            // TODO
        }

        fn limit() {
            // TODO
        }

        fn multiple_options() {
            // TODO
        }
    }

    mod get {
        use super::*;

        #[tokio::test]
        async fn existing() {
            let mut manager = setup().await;

            let character = Character::get(&mut manager, 1).await.unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "id" = ?"###);
            assert_debug_snapshot!(character, @r###"
            Character {
                id: 1,
                name: "Himmly",
                is_handsome: false,
                father_name: Some(
                    "Gloyne",
                ),
            }
            "###);
        }

        #[tokio::test]
        async fn non_existing() {
            let mut manager = setup().await;

            let result = Character::get(&mut manager, 3).await;

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT "id", "name", "is_handsome", "father_name" FROM "character" WHERE "id" = ?"###);
            assert_debug_snapshot!(result, @r###"
            Err(
                RowNotFound,
            )
            "###);
        }
    }

    mod count {
        use super::*;

        #[tokio::test]
        async fn normal() {
            let mut manager = setup().await;
            let count = Character::count(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Set(FindOperator::Eq(true)),
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT COUNT (*) AS "cnt" FROM "character" WHERE "is_handsome" = ?"###);
            assert_debug_snapshot!(count, @"2");
        }

        #[tokio::test]
        async fn empty_cond() {
            let mut manager = setup().await;
            let count = Character::count(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT COUNT (*) AS "cnt" FROM "character""###);
            assert_debug_snapshot!(count, @"3");
        }
    }

    mod exists {
        use super::*;

        #[tokio::test]
        async fn existing() {
            let mut manager = setup().await;
            let exists = Character::exists(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Set(FindOperator::Eq("Himmly".into())),
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT COUNT (*) AS "cnt" FROM "character" WHERE "name" = ?"###);
            assert_debug_snapshot!(exists, @"true");
        }

        #[tokio::test]
        async fn non_existing() {
            let mut manager = setup().await;
            let exists = Character::exists(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Set(FindOperator::Eq("Gollum".into())),
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT COUNT (*) AS "cnt" FROM "character" WHERE "name" = ?"###);
            assert_debug_snapshot!(exists, @"false");
        }

        #[tokio::test]
        async fn empty_cond() {
            let mut manager = setup().await;
            let exists = Character::exists(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"SELECT COUNT (*) AS "cnt" FROM "character""###);
            assert_debug_snapshot!(exists, @"true");
        }
    }
}

mod create {
    use super::*;

    mod insert {
        // TODO
        fn normal() {}

        fn partial_inputs() {}

        fn empty_inputs() {}

        fn different_fields_across_inputs() {}
    }

    mod create {
        // TODO
        use super::*;

        #[tokio::test]
        async fn normal() {
            // TODO
        }

        fn partial_inputs() {
            // TODO
        }

        fn empty_inputs() {
            // TODO
        }

        fn different_fields_across_inputs() {
            // TODO
        }
    }

    mod create_many {
        fn normal() {}

        fn no_inputs() {}
    }

    mod persist {
        fn normal() {}
    }
}

mod update {
    use super::*;
    mod update {
        use super::*;

        #[tokio::test]
        async fn normal() {
            let mut manager = setup().await;

            Character::update(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Set(FindOperator::Eq(1)),
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
                CharacterPatch {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Set(true),
                    father_name: Field::Omit,
                },
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"UPDATE "character" SET "is_handsome" = ? WHERE "id" = ?"###);

            let updated = Character::get(&mut manager, 1).await.unwrap();
            assert_debug_snapshot!(updated, @r###"
            Character {
                id: 1,
                name: "Himmly",
                is_handsome: true,
                father_name: Some(
                    "Gloyne",
                ),
            }
            "###);
        }

        #[tokio::test]
        async fn empty_cond() {
            let mut manager = setup().await;

            Character::update(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
                CharacterPatch {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Set(false),
                    father_name: Field::Omit,
                },
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"UPDATE "character" SET "is_handsome" = ?"###);

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .all()
            .await
            .unwrap();

            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: false,
                    father_name: None,
                },
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: Some(
                        "Gloyne",
                    ),
                },
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: false,
                    father_name: Some(
                        "Arathorn",
                    ),
                },
            ]
            "###);
        }

        #[tokio::test]
        async fn multiple_fields() {
            let mut manager = setup().await;

            Character::update(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
                CharacterPatch {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Set(false),
                    father_name: Field::Set(None),
                },
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"UPDATE "character" SET "is_handsome" = ?, "father_name" = ?"###);

            let characters = Character::find(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .all()
            .await
            .unwrap();

            assert_debug_snapshot!(characters, @r###"
            [
                Character {
                    id: 0,
                    name: "Legalas",
                    is_handsome: false,
                    father_name: None,
                },
                Character {
                    id: 1,
                    name: "Himmly",
                    is_handsome: false,
                    father_name: None,
                },
                Character {
                    id: 2,
                    name: "Aragorn",
                    is_handsome: false,
                    father_name: None,
                },
            ]
            "###);
        }
    }

    mod patch {
        use super::*;

        #[tokio::test]
        async fn normal() {
            let mut manager = setup().await;
            let mut character = Character::get(&mut manager, 1).await.unwrap();
            character
                .patch(
                    &mut manager,
                    CharacterPatch {
                        id: Field::Omit,
                        name: Field::Omit,
                        is_handsome: Field::Set(true),
                        father_name: Field::Omit,
                    },
                )
                .await
                .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"UPDATE "character" SET "is_handsome" = ? WHERE "id" = ?"###);
            assert_debug_snapshot!(character, @r###"
            Character {
                id: 1,
                name: "Himmly",
                is_handsome: true,
                father_name: Some(
                    "Gloyne",
                ),
            }
            "###);
            let updated_character = Character::get(&mut manager, 1).await.unwrap();
            assert_debug_snapshot!(updated_character, @r###"
            Character {
                id: 1,
                name: "Himmly",
                is_handsome: true,
                father_name: Some(
                    "Gloyne",
                ),
            }
            "###);
        }
    }
}

mod delete {
    use super::*;

    mod delete {
        use super::*;

        #[tokio::test]
        async fn normal() {
            let mut manager = setup().await;

            Character::delete(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Set(FindOperator::Eq(true)),
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"DELETE FROM "character" WHERE "is_handsome" = ?"###);

            let count = Character::count(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_debug_snapshot!(count, @"1");
        }

        #[tokio::test]
        async fn empty_cond() {
            let mut manager = setup().await;

            Character::delete(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"DELETE FROM "character""###);

            let count = Character::count(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_debug_snapshot!(count, @"0");
        }
    }

    mod remove {
        use super::*;

        #[tokio::test]
        async fn normal() {
            let mut manager = setup().await;

            let character = Character::get(&mut manager, 1).await.unwrap();
            character.remove(&mut manager).await.unwrap();

            assert_snapshot!(manager.last_statement().unwrap(), @r###"DELETE FROM "character" WHERE "id" = ?"###);

            let count = Character::count(
                &mut manager,
                vec![CharacterSelector {
                    id: Field::Omit,
                    name: Field::Omit,
                    is_handsome: Field::Omit,
                    father_name: Field::Omit,
                }],
            )
            .await
            .unwrap();

            assert_debug_snapshot!(count, @"2");
        }
    }
}
