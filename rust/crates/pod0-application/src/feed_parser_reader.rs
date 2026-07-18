use std::io::Cursor;

use pod0_domain::PodcastId;
use quick_xml::Reader;
use quick_xml::events::Event;
use url::Url;

use crate::feed::FeedParseFailure;
use crate::feed_parser::{ParsedRss, Parser};
use crate::feed_parser_values::{name, name_end};

pub(crate) fn parse_rss(
    bytes: &[u8],
    base_url: &Url,
    podcast_id: PodcastId,
) -> Result<ParsedRss, FeedParseFailure> {
    let mut parser = Parser::new(base_url, podcast_id);
    let mut reader = Reader::from_reader(Cursor::new(bytes));
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => parser.start(&element),
            Ok(Event::Empty(element)) => {
                parser.start(&element);
                parser.end(name(&element));
            }
            Ok(Event::Text(text)) => {
                let decoded = text.decode().map_err(|_| FeedParseFailure::MalformedXml)?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map_err(|_| FeedParseFailure::MalformedXml)?;
                parser.text(&unescaped);
            }
            Ok(Event::CData(text)) => parser.text(&String::from_utf8_lossy(&text)),
            Ok(Event::End(element)) => parser.end(name_end(&element)),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err(FeedParseFailure::MalformedXml),
        }
        buffer.clear();
    }
    parser.finish()
}
