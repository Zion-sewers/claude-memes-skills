# Clip Sourcing — Claude + YouTube + yt-dlp

How I found and cropped 49 meme clips for this system.

## The Problem

You need short (3-8 second) clips that:
- Start and end at exactly the right moment
- Feel immediately recognizable
- Match the emotional register of the workflow event

Finding, trimming, and cataloguing 49 of these manually would take a day. With Claude + yt-dlp it took about 3 hours.

## The Workflow

### Step 1: Describe the emotion to Claude

Don't search YouTube directly. Describe what you want:

> "I need a 4-5 second clip that captures the feeling of 'I told you I could do it' after tests finally pass. What are some well-known pop culture moments for this?"

Claude responds with a ranked list: LeBron James courtside "too easy", Cristiano Ronaldo SIUUU, Ron Burgundy "kind of a big deal", etc.

> "I need something for consecutive test failures — the feeling of 'every fix creates a new problem'. What's the most recognizable scene for that?"

Claude suggests Hydra (MCU), Whack-a-Mole, "This Is Fine" dog, etc.

### Step 2: Validate on YouTube

Search for Claude's suggestions. For each candidate:
- Watch the full clip
- Identify the exact start/end timestamp of the 4-6 second window you want
- Check it's available on YouTube (public, not geo-blocked)
- Prefer YouTube Shorts — they're already short and yt-dlp captures them cleanly

### Step 3: Add to memes.json

For YouTube Shorts (clip starts at 0:00):
```json
"tests_all_pass": {
  "name": "LeBron - That's Too Easy",
  "url": "https://www.youtube.com/shorts/JfMFxPqmD_E",
  "duration": 4,
  "trigger": "Test suite passes (all green)",
  "quote": "C'mon man, that's too easy!"
}
```

For longer videos where your clip starts mid-video, add a `start` field (seconds):
```json
"matrix_dodge": {
  "name": "Matrix - Bullet Dodge",
  "url": "https://www.youtube.com/watch?v=aHWriz1hBIY",
  "start": 312,
  "duration": 5,
  "trigger": "Test recovery (fail → pass)",
  "quote": "Whoa."
}
```

The `download-memes.sh` script reads `start` and computes the section range:
```bash
SECTIONS="*${start}-$((start + duration))"
```

### Step 4: Run download-memes.sh

```bash
~/.claude/bin/download-memes.sh
```

Output:
```
DOWNLOADING tests_all_pass ...
  OK: 1.2M tests_all_pass.mp4
DOWNLOADING matrix_dodge ...
  OK: 800K matrix_dodge.mp4
```

### Step 5: Test the clip

```bash
ffplay -autoexit ~/.claude/assets/memes/tests_all_pass.mp4
```

If the timing is off, adjust `start`/`duration` in memes.json and delete + re-download.

---

## Direct yt-dlp Commands (if you prefer manual control)

**Download a YouTube Short (full clip):**
```bash
yt-dlp \
  --format 'best[height<=480]' \
  --output ~/.claude/assets/memes/my_event.mp4 \
  "https://www.youtube.com/shorts/JfMFxPqmD_E"
```

**Download seconds 5–10 of a longer video:**
```bash
yt-dlp \
  --format 'best[height<=480]' \
  --download-sections "*5-10" \
  --force-keyframes-at-cuts \
  --output ~/.claude/assets/memes/my_event.mp4 \
  "https://www.youtube.com/watch?v=XXXX"
```

**Re-encode to a fixed 400×260 size (optional, for consistent window):**
```bash
ffmpeg -i ~/.claude/assets/memes/my_event.mp4 \
  -vf "scale=400:260:force_original_aspect_ratio=decrease,pad=400:260:(ow-iw)/2:(oh-ih)/2" \
  -c:a copy \
  ~/.claude/assets/memes/my_event_scaled.mp4
```

---

## Clip Selection Guidelines

| Event type | Clip feel | Examples |
|------------|-----------|---------|
| Success after struggle | "Finally. I earned this." | LeBron courtside, SIUUU |
| Clean first pass | "That was effortless" | Jonah Hill thumbs up |
| Consecutive failure | "Every fix breaks something new" | Hydra, Whack-a-Mole |
| Recovery from failure | "Narrow escape" | Matrix dodge, last-second save |
| Scope creep / large diff | "What have you done" | HIMYM slap, Thanos snap |
| Friday deploy | "You absolute fool" | Any panic/danger clip |
| Desperation (stuck in loop) | "Everything is fine (it's not)" | This Is Fine dog |
| Session start | "Let's get to work" | Warcraft peon, any "assembly" |
| Refactor loop | "I've done this before" | Dormammu bargain |

## Notes on Copyright

- Clips are 3-8 seconds of copyrighted content used locally for personal workflow feedback.
- Don't redistribute the clips in the repo — link to `memes.json` + `download-memes.sh` instead.
- YouTube ToS permits personal, non-commercial use for clips consumed on your own machine.
- The `config/memes.json` in this repo ships with placeholder URLs — fill in your own.
