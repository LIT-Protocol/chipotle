use lit_agent_keychain::{billing, crypto};
use sqlx::postgres::PgPoolOptions;
#[tokio::test]
async fn execution_budget_is_atomic_and_never_overdrawn() {
    let Ok(url) = std::env::var("KEYCHAIN_TEST_DATABASE_URL") else {
        return;
    };
    assert!(
        url.contains("127.0.0.1") || url.contains("localhost"),
        "test database must be local"
    );
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&url)
        .await
        .unwrap();
    let bucket = format!("test-{}", crypto::random_token());
    let mut tasks = Vec::new();
    for _ in 0..30 {
        let pool = pool.clone();
        let bucket = bucket.clone();
        tasks.push(tokio::spawn(async move {
            billing::reserve(&pool, &bucket, 3600, 7).await.is_ok()
        }));
    }
    let mut accepted = 0;
    for task in tasks {
        if task.await.unwrap() {
            accepted += 1;
        }
    }
    assert_eq!(accepted, 7);
    let used: i64 = sqlx::query_scalar("SELECT used FROM kc_budgets WHERE bucket=$1")
        .bind(&bucket)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(used, 7);
    sqlx::query("DELETE FROM kc_budgets WHERE bucket=$1")
        .bind(&bucket)
        .execute(&pool)
        .await
        .unwrap();
}
