# Bugs found while benchmarking against rekordbox (2026-10-23)

| # | Bug | Status |
|---|---|---|
| 1 | Streaming-only XML entries (`file://localhost/v4/catalog/tracks/...`) were treated as files; loading one showed a raw "could not open" error. | Fixed: they load as `Streaming` with no file, are dimmed in the list, and loading one explains why. |
| 2 | All Tracks showed 112 where rekordbox shows 108. | Not a bug: the XML export (112 entries) predates changes to the rekordbox library; rekordbox counts streaming tracks too. |
| 3 | Player A gave no feedback while loading; the old track stayed for ~700 ms. | Fixed: the player clears at once, as rekordbox's does, and the old track returns if the load fails. |
| 4 | Idle CPU: the window redrew at 60 fps all the time (41.5% of a core). | Fixed: frame-rate redraws only while something moves; idle is 1.8%. |
| 5 | Every load decoded and re-analysed the track. | Fixed: library grid and key are used without analysis; analysis results are cached on disk (`~/Library/Caches/Cuepoint/analysis`). |
| 6 | In-app loads took ~2.5x the raw decode and analysis time. | Fixed: decoding at the file's rate, then resampling (per channel, in parallel) alongside the analysis. Click to drawn went from 711-792 ms to 434-489 ms (rekordbox: 571-594 ms). |
| 7 | On 48 kHz output every load resampled the whole track. | Kept, but made cheap: about 93 ms, overlapped with analysis. |
| 8 | Rows highlighted on mouse release (63-77 ms after the press; rekordbox 15-43 ms). | Fixed: rows highlight on press (23-35 ms). |
