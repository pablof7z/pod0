# Snipd Feature Model For Podcastr

> Competitive research note for product requirements. Sources checked on 2026-05-09.

## Source Confidence

Snipd's public pages and help docs expose product behavior, not implementation internals. The "how it works" section below is therefore a product/architecture inference: it names the pipeline Podcastr should implement to reproduce and improve the user-visible behavior, without assuming Snipd's private code or vendors.

## Publicly Observed Snipd Features

### Episode Intelligence Substrate

Snipd's core surface depends on episodes being AI-processed. Public docs describe transcripts with speaker identification, AI-generated chapters, episode summaries, AI snips, chat, and mentioned-book extraction. A troubleshooting page states that snips can fall back to plain 1-minute timestamps when transcripts, speaker identification, and summaries are not yet available, and asks premium users to run AI processing before rewriting the snip.

The key product lesson: "processed episode" must be a first-class state in Podcastr. Every smart surface should know whether it has publisher metadata only, transcript only, transcript plus speaker IDs, transcript plus entities, or the full compiled knowledge pass.

### Headphone And Car Snipping

Snipd maps the headphone skip-back control to snip creation. The help docs say the trigger captures and summarizes the relevant segment, preserving transcript and speaker names, and plays a confirmation beep. CarPlay uses the same idea: steering-wheel skip-back creates a snip, and AI identifies the right segment and writes the note. Auto-snipping can also capture key moments while the user simply listens.

Podcastr should keep the same ambient capture principle, but avoid permanently stealing the user's normal skip-back muscle memory. Requirement: support at least two configurable remote-command mappings, with "skip back still skips" as a safe default and "hold/double/triple action creates snip" as the power-user mode.

### Snip Artifact Shape

Snipd lets users customize generated titles, summary format, custom prompts, and snip duration. Duration can be automatic, where the AI decides the context window and tries to find the best start/end point, or fixed from the trigger time.

Podcastr's `Snip` model should therefore be more than a bookmark:

```text
Snip {
  id
  episode_id
  trigger_time_ms
  start_ms
  end_ms
  transcript_span_ids
  speaker_ids
  generated_title
  generated_summary
  user_note
  prompt_profile_id
  duration_mode: auto | fixed
  source: headphone | carplay | watch | touch | auto | siri | agent
}
```

### Auto-Chapters And Listening Compression

Snipd markets AI-generated chapters for overview/navigation, chapter skipping from headphones, and skip-intro/outro based on AI chapters. Its AI DJ feature goes further: it takes control of playback, jumps through the most valuable original moments, and bridges them with short spoken context. The public help page says AI DJ covers about one quarter of the original episode and is not just a summary because the user still hears the original voices.

Podcastr should distinguish three related but separate artifacts:

- **Chapters**: navigational sections, publisher-first and AI fallback.
- **Highlights**: ranked moments worth saving or surfacing before listening.
- **DJ path**: an ordered playable route through original audio, with generated bridges and source anchors.

This gives us a stronger version of Snipd: the existing briefing system can produce both narrated synthetic episodes and "guided original-audio routes."

### Mentioned Books

Snipd's mentioned-books release says it identifies books in each processed episode and stores title, author, cover, description, purchase links, and episode context. Books appear on episode screens and in the snips library, and Snipd also exposes public "most mentioned books" and per-show top-book pages.

Podcastr should treat books as first-class entities in the knowledge layer:

- Extract mentions from transcript spans.
- Resolve title/author to canonical book IDs.
- Attach context: recommendation, counterargument, background reading, citation, disagreement, joke, or passing mention.
- Build show-level and library-level top-book pages.
- Link every book card back to playable timestamp spans.

### Guests And Similar Guests

Snipd exposes guest pages with bios, episode appearances, snip counts, pictures on the App Store listing, followable favorites, and similar-guest discovery. Public guest pages rank podcast appearances for a person and include AI-style bios.

Podcastr already has speaker/topic profiles; this research tightens the requirement. Guest profiles must not be only local transcript names. They need:

- Identity resolution from RSS/show notes, transcript NER, speaker diarization, and user corrections.
- Bio, portrait attribution, aliases, roles, and confidence.
- Appearance list across the user's library and optionally public Snipd-like discovery data.
- Similar people by topic embedding, co-appearance graph, book/topic overlap, and listener behavior if available.
- Follow events that feed Today and notifications.

### Export And Review Loop

Snipd's features include sharing snips as text, audio, video, or quote cards; daily recap/spaced review; Readwise, Notion, Glasp, Obsidian, Bear, Logseq, and Markdown export; and AI support for uploaded audio and YouTube.

Podcastr should preserve its current wiki-first direction but make saved moments portable. A snip should be exportable as Markdown with frontmatter, transcript quote, summary, speaker, episode metadata, and a deep link back to the timestamp.

## Inferred Architecture For Podcastr

1. **Episode processing graph**: RSS/publisher metadata -> transcript source -> diarization -> language -> chapters -> summary -> highlights -> entities -> embeddings -> wiki pages.
2. **Span-first data model**: every generated artifact points to transcript span IDs and timestamps. If a span is missing, the UI must show that the artifact is not yet grounded.
3. **Entity extraction workers**: books, guests, topics, claims, quotes, and actions are separate extraction passes, not one prompt blob.
4. **Entity resolver layer**: merge noisy mentions into canonical `Book`, `Person`, and `Topic` records with confidence and provenance.
5. **Ambient event buffer**: headphone, Watch, CarPlay, Siri, Action Button, and touch triggers write capture intents immediately, then enrichment runs async.
6. **Playback route compiler**: chapters, highlights, and DJ paths are all ordered span lists over original audio, while briefings are generated audio artifacts.
7. **Export adapters**: Readwise/Notion/Obsidian-style exports should read from the same snip/wiki model, not custom one-off serializers.

## Product Decisions

- **v1 must include AI chapters, transcript search, snips, guest pages, and mentioned books** if we want credible Snipd parity on learning workflows.
- **v1 should include headphone/CarPlay snipping only if remote-command mapping is configurable**. Otherwise we risk breaking normal podcast controls.
- **AI DJ-style guided original-audio playback belongs in v1.1 unless briefing work absorbs it naturally**. It is high demo value but requires careful playback control and bridge TTS.
- **Mentioned books should share the same entity pipeline as topics and guests**. Do not build a books-only extraction path.
- **Guest similarity should start as local-library similarity**. Public/global similarity can wait until we have server-side aggregate data or an explicit public index.

## Sources

- Snipd feature overview: https://www.snipd.com/all-features
- Headphone snipping help: https://support.snipd.com/en/articles/10225450-create-snips-with-your-headphones
- CarPlay snipping help: https://support.snipd.com/en/articles/10225413-how-to-use-snipd-on-apple-carplay
- Snip customization help: https://support.snipd.com/en/articles/11926947-snip-customizations
- AI DJ help: https://support.snipd.com/en/articles/14287414-snipd-s-ai-dj
- Mentioned books release: https://www.snipd.com/blog/mentioned-books-release
- Popular guests page: https://share.snipd.com/popular-guests
- Example guest page: https://share.snipd.com/person/marc-andreessen/RyltOuyyQ8SCP7SFLGBZdg
- Most mentioned books page: https://share.snipd.com/podbooks
- Uploads help: https://support.snipd.com/en/articles/10226381-upload-your-own-audio-files
- Missing AI summaries troubleshooting: https://support.snipd.com/en/articles/10537507-why-are-my-snips-missing-ai-summaries-and-only-show-1min-snip
- App Store listing: https://apps.apple.com/us/app/snipd-ai-podcast-player/id1557206126
