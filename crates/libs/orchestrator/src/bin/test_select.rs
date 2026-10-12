use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (tts_tx, mut tts_rx) = mpsc::channel::<String>(50);
    let (clear_tx, mut clear_rx) = mpsc::channel::<()>(10);

    tokio::spawn(async move {
        println!("Worker started");
        let mut worker_running = true;
        while worker_running {
            let tts_rx_ref = &mut tts_rx;
            let clear_rx_ref = &mut clear_rx;

            loop {
                tokio::select! {
                    Some(_) = clear_rx_ref.recv() => {
                        println!("Clear received");
                    }
                    text_opt = tts_rx_ref.recv() => {
                        match text_opt {
                            Some(text) => {
                                println!("Received text: {}", text);
                                tokio::select! {
                                    Some(_) = clear_rx_ref.recv() => {
                                        println!("Inner clear received");
                                    }
                                    _ = synthesize(&text) => {
                                        println!("Synthesize completed");
                                    }
                                }
                            }
                            None => {
                                println!("TTS rx closed");
                                worker_running = false;
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    tts_tx.send("Hello world".to_string()).await.unwrap();
    println!("Sent greeting");
    
    // Hold clear_tx alive
    let _clear = clear_tx;
    
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}

async fn synthesize(text: &str) {
    println!("Synthesizing: {}", text);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}
