use crate::{AppState, ConversationEntry, InputMode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use comrude_core::{MessageSender, MessageContent};
use std::collections::VecDeque;

pub fn draw_ui(frame: &mut Frame, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Conversation
            Constraint::Length(3),  // Input
            Constraint::Length(1),  // Status
        ])
        .split(frame.size());

    draw_header(frame, chunks[0]);
    draw_conversation(frame, chunks[1], app);
    draw_input(frame, chunks[2], app);
    draw_status(frame, chunks[3], app);
}

fn draw_header(frame: &mut Frame, area: ratatui::layout::Rect) {
    let title = Paragraph::new("Comrude - Universal AI Development Assistant")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );
    frame.render_widget(title, area);
}

fn draw_conversation(frame: &mut Frame, area: ratatui::layout::Rect, app: &AppState) {
    let conversation_future = app.conversation.try_read();
    let conversation = match conversation_future {
        Ok(conv) => conv,
        Err(_) => {
            // Handle case where we can't get read lock
            let empty_conversation = VecDeque::new();
            return draw_conversation_items(frame, area, &empty_conversation);
        }
    };

    draw_conversation_items(frame, area, &*conversation);
}

fn draw_conversation_items(
    frame: &mut Frame, 
    area: ratatui::layout::Rect, 
    conversation: &VecDeque<ConversationEntry>
) {
    let mut items = Vec::new();

    for entry in conversation.iter() {
        // Add user message
        let user_style = Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);
        let user_prefix = match entry.message.sender {
            MessageSender::User => "You: ",
            MessageSender::System => "System: ",
            MessageSender::Assistant { .. } => "Assistant: ",
        };

        let user_content = match &entry.message.content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Code { content, language } => {
                format!("```{}\n{}\n```", language, content)
            }
            MessageContent::File { path, preview } => {
                if let Some(preview_content) = preview {
                    format!("File: {}\n{}", path, preview_content)
                } else {
                    format!("File: {}", path)
                }
            }
            MessageContent::Error { error_type, message } => {
                format!("Error ({}): {}", error_type, message)
            }
            MessageContent::Progress { stage, percentage } => {
                format!("Progress: {} ({}%)", stage, percentage)
            }
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(user_prefix, user_style),
            Span::raw(user_content),
        ])));

        // Add spacing after user message
        items.push(ListItem::new(Line::from(""))); 

        // Add assistant response if available
        if let Some(response) = &entry.response {
            let assistant_style = Style::default().fg(Color::Blue);
            items.push(ListItem::new(Line::from(vec![
                Span::styled("Assistant: ", assistant_style.add_modifier(Modifier::BOLD)),
                Span::styled(&response.content, assistant_style),
            ])));

            // Add token usage info if available
            let usage = &response.tokens_used;
            let usage_style = Style::default().fg(Color::Gray);
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("(Tokens: {} prompt + {} completion = {})", 
                        usage.prompt_tokens, 
                        usage.completion_tokens, 
                        usage.total_tokens
                    ),
                    usage_style
                ),
            ])));
            
            // Add spacing after assistant response
            items.push(ListItem::new(Line::from(""))); 
        }

        // Add extra spacing between conversation entries
        items.push(ListItem::new(Line::from(""))); 
    }

    let conversation_list = List::new(items)
        .block(
            Block::default()
                .title("Conversation")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );

    frame.render_widget(conversation_list, area);
}

fn draw_input(frame: &mut Frame, area: ratatui::layout::Rect, app: &AppState) {
    let input_style = match app.input_mode {
        InputMode::Normal => Style::default(),
        InputMode::Insert => Style::default().fg(Color::Yellow),
        InputMode::Command => Style::default().fg(Color::Magenta),
    };

    let input_prefix = match app.input_mode {
        InputMode::Normal => "Normal: ",
        InputMode::Insert => "Insert: ",
        InputMode::Command => "Command: ",
    };

    let input_text = format!("{}{}", input_prefix, app.get_input());
    
    let input = Paragraph::new(input_text)
        .style(input_style)
        .block(
            Block::default()
                .title("Input")
                .borders(Borders::ALL)
                .border_style(match app.input_mode {
                    InputMode::Normal => Style::default().fg(Color::Blue),
                    InputMode::Insert => Style::default().fg(Color::Yellow),
                    InputMode::Command => Style::default().fg(Color::Magenta),
                }),
        );

    frame.render_widget(input, area);

    // Set cursor position
    if matches!(app.input_mode, InputMode::Insert | InputMode::Command) {
        frame.set_cursor(
            area.x + input_prefix.len() as u16 + app.get_input().len() as u16 + 1,
            area.y + 1,
        );
    }
}

fn draw_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &AppState) {
    let status_text = app
        .status_message
        .as_ref()
        .map(|msg| msg.as_str())
        .unwrap_or("Ready");

    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Gray));

    frame.render_widget(status, area);
}