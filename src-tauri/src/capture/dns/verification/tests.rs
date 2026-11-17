use super::*;

fn tun() -> GuiTunStatus {
    GuiTunStatus {
        name: Some("utun7".into()),
        enabled: true,
        healthy: true,
        dns_hijacked_queries: 8,
        ..Default::default()
    }
}

#[tokio::test]
async fn capture_publication_and_repeated_recovery_require_increasing_packet_observations() {
    let before = tun();
    let mut observed = before.clone();
    for _ in 0..4 {
        let previous = observed.clone();
        observed.dns_hijacked_queries += 1;
        verify(&previous, async { Ok(()) }, || async {
            Ok(observed.clone())
        })
        .await
        .unwrap();
    }
    assert!(matches!(
        verify(&observed, async { Ok(()) }, || async {
            Ok(observed.clone())
        })
        .await,
        Err(Failure::Confirmed(_))
    ));
    let mut replaced = observed.clone();
    replaced.name = Some("utun8".into());
    replaced.dns_hijacked_queries += 1;
    assert!(matches!(
        verify(&observed, async { Ok(()) }, || async { Ok(replaced) }).await,
        Err(Failure::Confirmed(_))
    ));
}
#[tokio::test]
async fn unreachable_peer_is_definite_but_status_timeout_is_unknown() {
    assert!(matches!(
        verify(
            &tun(),
            async { Err(AppError::internal("packet route unreachable")) },
            || async { panic!("must not fabricate a status observation") }
        )
        .await,
        Err(Failure::Confirmed(_))
    ));
    assert!(matches!(
        verify(&tun(), async { Ok(()) }, || async {
            Err(AppError::internal("IPC timed out"))
        })
        .await,
        Err(Failure::Unknown(_))
    ));
}
