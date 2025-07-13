use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub enum AppEvent {
    Tick,
    Key(KeyCode, KeyModifiers),
    Resize(u16, u16),
    Quit,
}

pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        Self { tick_rate }
    }

    pub async fn next_event(&self) -> Result<AppEvent, Box<dyn std::error::Error>> {
        // Use a smaller timeout to make the app more responsive
        let poll_timeout = std::cmp::min(self.tick_rate, Duration::from_millis(50));
        
        match event::poll(poll_timeout) {
            Ok(true) => {
                match event::read() {
                    Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        // Handle Ctrl+C for graceful shutdown
                        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                            return Ok(AppEvent::Quit);
                        }
                        Ok(AppEvent::Key(key.code, key.modifiers))
                    }
                    Ok(Event::Resize(width, height)) => Ok(AppEvent::Resize(width, height)),
                    Ok(_) => Ok(AppEvent::Tick),
                    Err(e) => {
                        eprintln!("Error reading event: {}", e);
                        Ok(AppEvent::Tick)
                    }
                }
            }
            Ok(false) => Ok(AppEvent::Tick),
            Err(e) => {
                eprintln!("Error polling events: {}", e);
                Err(e.into())
            }
        }
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new(Duration::from_millis(250))
    }
}