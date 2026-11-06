//! Rendered Markdown, built from the COSMIC widgets.

use crate::app::Message;
use cosmic::iced::widget::{rich_text, span};
use cosmic::iced::{Font, Length, font};
use cosmic::widget::{self, column, container, row, scrollable, text};
use cosmic::{Element, theme};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Default)]
pub struct Inline {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: bool,
    pub strike: bool,
}

#[derive(Debug, Clone)]
pub enum Block {
    Heading(u8, Vec<Inline>),
    Para(Vec<Inline>),
    Code(String),
    Item { depth: usize, marker: String, content: Vec<Inline> },
    Quote(Vec<Inline>),
    Rule,
    Row { cells: Vec<Vec<Inline>>, header: bool },
}

pub fn parse(src: &str) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut out = Vec::new();
    let mut cur: Vec<Inline> = Vec::new();
    let (mut bold, mut italic, mut link, mut strike) = (0, 0, 0, 0);
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut in_quote = 0;
    let mut code: Option<String> = None;
    let mut cells: Vec<Vec<Inline>> = Vec::new();
    let mut in_head = false;
    let push_text = |cur: &mut Vec<Inline>, t: &str, code: bool, bold: i32, italic: i32, link: i32, strike: i32| {
        cur.push(Inline {
            text: t.to_string(),
            bold: bold > 0,
            italic: italic > 0,
            code,
            link: link > 0,
            strike: strike > 0,
        });
    };
    for ev in Parser::new_ext(src, opts) {
        match ev {
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold -= 1,
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic -= 1,
            Event::Start(Tag::Strikethrough) => strike += 1,
            Event::End(TagEnd::Strikethrough) => strike -= 1,
            Event::Start(Tag::Link { .. }) => link += 1,
            Event::End(TagEnd::Link) => link -= 1,
            Event::Start(Tag::CodeBlock(_)) => code = Some(String::new()),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(c) = code.take() {
                    out.push(Block::Code(c.trim_end().to_string()));
                }
            }
            Event::Start(Tag::List(first)) => lists.push(first),
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::BlockQuote(_)) => in_quote += 1,
            Event::End(TagEnd::BlockQuote(_)) => in_quote -= 1,
            Event::End(TagEnd::Heading(level)) => {
                let l = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    _ => 5,
                };
                out.push(Block::Heading(l, std::mem::take(&mut cur)));
            }
            Event::End(TagEnd::Paragraph) => {
                if lists.is_empty() {
                    let c = std::mem::take(&mut cur);
                    out.push(if in_quote > 0 { Block::Quote(c) } else { Block::Para(c) });
                }
            }
            Event::Start(Tag::Item) => {
                cur.clear();
            }
            Event::End(TagEnd::Item) => {
                let depth = lists.len().saturating_sub(1);
                let marker = match lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}.");
                        *n += 1;
                        m
                    }
                    _ => "•".to_string(),
                };
                let content = std::mem::take(&mut cur);
                out.push(Block::Item { depth, marker, content });
            }
            Event::TaskListMarker(done) => {
                cur.insert(0, Inline { text: if done { "☑ ".into() } else { "☐ ".into() }, ..Default::default() });
            }
            Event::Start(Tag::TableHead) => {
                in_head = true;
                cells.clear();
            }
            Event::End(TagEnd::TableHead) => {
                out.push(Block::Row { cells: std::mem::take(&mut cells), header: true });
                in_head = false;
            }
            Event::End(TagEnd::TableRow) => {
                out.push(Block::Row { cells: std::mem::take(&mut cells), header: in_head });
            }
            Event::End(TagEnd::TableCell) => cells.push(std::mem::take(&mut cur)),
            Event::Text(t) => {
                if let Some(c) = &mut code {
                    c.push_str(&t);
                } else {
                    push_text(&mut cur, &t, false, bold, italic, link, strike);
                }
            }
            Event::Code(t) => push_text(&mut cur, &t, true, bold, italic, link, strike),
            Event::SoftBreak => push_text(&mut cur, " ", false, bold, italic, link, strike),
            Event::HardBreak => push_text(&mut cur, "\n", false, bold, italic, link, strike),
            Event::Rule => out.push(Block::Rule),
            Event::Html(t) | Event::InlineHtml(t) => push_text(&mut cur, &t, true, 0, 0, 0, 0),
            _ => {}
        }
    }
    if !cur.is_empty() {
        out.push(Block::Para(cur));
    }
    out
}

fn inlines<'a>(items: &'a [Inline], size: f32, mono: Font, accent: cosmic::iced::Color) -> Element<'a, Message> {
    let spans: Vec<_> = items
        .iter()
        .map(|i| {
            let mut s = span::<(), Font>(i.text.as_str()).size(size);
            let mut f = cosmic::font::default();
            if i.bold {
                f.weight = font::Weight::Bold;
            }
            if i.italic {
                f.style = font::Style::Italic;
            }
            if i.code {
                f = mono;
                s = s.background(cosmic::iced::Color { a: 0.12, ..accent }).padding([0, 3]);
            }
            s = s.font(f);
            if i.link {
                s = s.color(accent).underline(true);
            }
            if i.strike {
                s = s.strikethrough(true);
            }
            s
        })
        .collect();
    rich_text(spans).width(Length::Fill).into()
}

pub fn view<'a>(blocks: &'a [Block], mono: Font) -> Element<'a, Message> {
    let accent: cosmic::iced::Color = cosmic::theme::active().cosmic().accent.base.into();
    let mut col = column::with_capacity(blocks.len()).spacing(10).padding([20, 28]).width(Length::Fill);
    for b in blocks {
        let el: Element<'a, Message> = match b {
            Block::Heading(l, c) => {
                let size = match l {
                    1 => 28.0,
                    2 => 23.0,
                    3 => 19.0,
                    4 => 16.0,
                    _ => 14.0,
                };
                let content = container(inlines(c, size, mono, accent)).padding([6, 0, 0, 0]);
                if *l <= 2 {
                    column::with_children(vec![content.into(), widget::divider::horizontal::light().into()]).spacing(6).into()
                } else {
                    content.into()
                }
            }
            Block::Para(c) => inlines(c, 14.0, mono, accent),
            Block::Quote(c) => container(inlines(c, 14.0, mono, accent)).padding([4, 0, 4, 14]).class(theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    border: cosmic::iced::Border { color: cosmic::iced::Color { a: 0.5, ..accent }, width: 1.0, radius: 2.0.into() },
                    ..Default::default()
                }
            })).into(),
            Block::Code(c) => container(text(c.as_str()).font(mono).size(13.0).width(Length::Fill))
                .padding(12)
                .width(Length::Fill)
                .class(theme::Container::Card)
                .into(),
            Block::Item { depth, marker, content } => row::with_children(vec![
                widget::Space::new().width(Length::Fixed(*depth as f32 * 18.0)).into(),
                text(marker.as_str()).size(14.0).width(Length::Fixed(22.0)).into(),
                inlines(content, 14.0, mono, accent),
            ])
            .into(),
            Block::Rule => widget::divider::horizontal::default().into(),
            Block::Row { cells, header } => {
                let mut r = row::with_capacity(cells.len()).spacing(12);
                for c in cells {
                    let cell: Element<'a, Message> = if *header {
                        let bold: Vec<Inline> = c.iter().cloned().map(|mut i| { i.bold = true; i }).collect();
                        // Header text is short: a plain owned label is simplest here.
                        text(bold.iter().map(|i| i.text.clone()).collect::<String>())
                            .font(cosmic::font::bold())
                            .size(14.0)
                            .width(Length::Fill)
                            .into()
                    } else {
                        inlines(c, 14.0, mono, accent)
                    };
                    r = r.push(container(cell).width(Length::Fill));
                }
                r.into()
            }
        };
        col = col.push(el);
    }
    scrollable(col).width(Length::Fill).height(Length::Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_blocks() {
        let b = parse("# T\n\ntext **b** `c`\n\n- a\n- b\n\n1. x\n\n```rs\nfn a() {}\n```\n\n> q\n\n---\n\n| A | B |\n|---|---|\n| 1 | 2 |\n");
        assert!(matches!(&b[0], Block::Heading(1, _)));
        let Block::Para(p) = &b[1] else { panic!("{:?}", b[1]) };
        assert!(p.iter().any(|i| i.bold && i.text == "b"));
        assert!(p.iter().any(|i| i.code && i.text == "c"));
        assert!(b.iter().filter(|x| matches!(x, Block::Item { .. })).count() == 3);
        assert!(b.iter().any(|x| matches!(x, Block::Item { marker, .. } if marker == "1.")));
        assert!(b.iter().any(|x| matches!(x, Block::Code(c) if c == "fn a() {}")));
        assert!(b.iter().any(|x| matches!(x, Block::Quote(_))));
        assert!(b.iter().any(|x| matches!(x, Block::Rule)));
        assert_eq!(b.iter().filter(|x| matches!(x, Block::Row { .. })).count(), 2);
    }
}
