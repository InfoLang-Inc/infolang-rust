//! Live integration probe against a configured gateway.
//! Run with: INFOLANG_LIVE_TEST=1 cargo test --test live_test -- --ignored

use infolang::Client;

#[tokio::test]
#[ignore = "requires INFOLANG_LIVE_TEST=1 and credentials"]
async fn live_investigate() {
    if std::env::var("INFOLANG_LIVE_TEST").ok().as_deref() != Some("1") {
        return;
    }
    let client = Client::builder().build().expect("client from env");
    let ready = client.ready().await.expect("readyz");
    assert_eq!(ready["ready"], true);
    let _ = client.investigate("InfoLang SDK live probe", None).await;
}
