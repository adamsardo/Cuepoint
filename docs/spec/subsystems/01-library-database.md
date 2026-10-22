# 01. Library and database

Status: first pass, static evidence only (classes.tsv, demangled symbols, strings, bundled resources, rekordboxAgent app.asar identifiers). Nothing here has been confirmed by decompilation yet. Inferences are labelled **(inference)**.

Evidence notation: backticks at the end of a bullet name a class, symbol, string or file that supports it. `str:` means a literal from strings.txt, `sym:` a demangled symbol, `agent:` an identifier recovered from the rekordboxAgent Electron bundle (`Contents/MacOS/rekordboxAgent.app/Contents/Resources/app.asar`), `res:` a bundled resource.

---

## 1. Purpose

Owns the user's collection: the encrypted SQLite `master.db` and everything that reads or writes it (tracks, playlists, folders, smart playlists, histories, tag/request lists, MyTags, hot cue bank lists, sampler/video lists, related-track criteria, trash), plus the file-level services around it: tag read/write, artwork cache, import/export (rekordbox XML, m3u/m3u8/pls/txt, iTunes/Music), search/filter/sort, related tracks and track suggestion, missing/duplicate file management, backup/restore/move, cloud library sync via the local rekordboxAgent, and migration from rekordbox 5 (DeviceSQL `datafile.edb`).

---

## 2. Feature inventory (parity checklist)

### 2.1 Database lifecycle

- Open the master database `master.db` (SQLCipher-encrypted SQLite) at start-up; key supplied with `sqlite3_key` before first use. `str:@executable_path/libsqlcipher.0.dylib`, `sym:SQLiteDB::open(juce::File const&, juce::String const&, bool)`, import `_sqlite3_key`
- Database lives in a "PIONEER/Master" style folder on the system or on a chosen drive (database can be moved to another drive). `str:.PIONEER/Master/`, `str:PIONEER/Master/`, `str:Move Master Database`, `str:Select a drive as a new location.`, `db::MoveLibraryThread`, `db::PrepMoveLibraryThread`, `db::CancelMoveLibraryThread`, `db::MoveLibrarySelector`
- Default per-user data folder (macOS) is under `~/Library/Pioneer/rekordbox/` for the DB and `~/Library/Application Support/Pioneer/rekordbox6/` for settings (**inference** from prior art plus `str:Application Support/Pioneer`, `str:Pioneer/rekordbox6`). `db::DBFileDef::getMasterDBDir`, `getMasterPioneerDir`, `getCloudDir`
- WAL journalling with synchronous NORMAL. `str:PRAGMA journal_mode = WAL`, `str:PRAGMA synchronous = NORMAL`, `str:master.db-wal`, `str:master.db-shm`
- Integrity check and broken-database quarantine: a DB that fails to open is moved into a `brokendb/<YYYY-MM-DD>` folder and the user is told. `str:PRAGMA integrity_check;`, `str:brokendb`, `BrokenDBHelper.cpp`, `sym:db::rekordboxDBController::removeBrokenMasterDBIfExist`, `str:Unable to open the database file.`
- "Master database missing" dialog if `master.db` is absent. `MasterDbMissingWindow.cpp`, `App/Service/MasterDBMissing/`
- Rolling automatic backups of `master.db`: `master.backup.db`, `master.backup2.db`, `master.backup3.db` and dated `master.backup.YYYYMMDD.db`, discovered with a regex. `str:master\.(backup(\d?)|backup\.(\d{4})(\d{2})(\d{2}))\.db`, `str:master.backup.%Y%m%d.db`, `db::DBFileDef::masterDbBackupFindAllFiles`, `masterDbBackupFormatFilename`, `getAutoBackupDBFileName`
- Backup copy is made with SQLCipher export into an attached keyed database (so backups stay encrypted). `str:ATTACH DATABASE '__BACKUP_DB__' AS backup KEY '__KEY__'; ... sqlcipher_export('backup') ...`, `sym:SQLiteDB::backup`
- Vacuum / "Database Cleanup": checks `freelist_count` × `page_size` and only vacuums above a threshold. `rb::middleware::db::rekordbox::MaintenanceRepository`, `str:Skip vacuum process. freelist_count:`, `str:Database Cleanup`, `str:Database cleanup complete`, `sym:DatabaseMediator::vacuumMasterDb`
- Schema upgrade on open: missing tables created (`CREATE TABLE IF NOT EXISTS` for trash tables), missing columns added (`alter table ... add column`, tolerating "duplicate column name"), missing indexes created (`djmd_cue__content_i_d_rb_local_deleted`). `str:CREATE INDEX \`djmd_cue__content_i_d_rb_local_deleted\``, `sym:SQLiteDB::addColumns`, `columnsExist`, `sym:rekordboxDBController::checkTable`, `initExtendedDB`
- Default/seed rows: menu items, categories, sorts, colours, keys, property row, root sampler/related-tracks folders, default MyTag groups ("Genre", "Components", "Situation" with seed values like "Acid House", "Main Floor", "Peak Time"). `sym:initMenuItemsTable`, `initCategoryTable`, `initSortTable`, `initPropertyTable`, `initSamplerList`, `initializeMyTag`, `str:Acid House`, `str:Peak Time`, `str:Situation`
- A template `master.db` (4096 bytes, encrypted) ships in the bundle. `res:Resources/master.db`
- Global DB access lock and analysis/backup locks so long jobs do not interleave. `db::rekordboxDBController::ScopedAccessLock`, `db::DsqlCriticalSectionLock`, `sym:DatabaseIF::analysisLock`, `backupAndRestoreLock`
- Product catalogue DB `product.db` (encrypted, separate password) maps hardware product IDs to names, base models, colour variants; used for device naming and registration. `rb::middleware::db::product::{ProductDB,ProductRepository,Product,Variant,ApiProduct}`, `ProductDB::getDbPassword`, `res:Resources/product.db`
- Other side databases: `networkRecommend.db` (cloud recommendation cache), `LightingDB/macro.db3`, `LightingDB/user.db3` (lighting subsystem). `str:networkRecommend.db`, `NetworkRecommend::DataBase`

### 2.2 Collection (tracks)

- Collection view of every non-deleted `djmdContent` row; counts with/without streaming tracks. `sym:DatabaseIF::getNumOfCollection`, `getNumOfCollectionWithoutStreaming`, `getAllCollection`
- Import tracks/files/folders (recursive folder scan), drag and drop, import as sampler track, import video files. `sym:DatabaseIF::importTrack`, `importFile`, `importFileAsSamplerTrack`, `importTrackAsSamplerTrack`, `importVideoFile`, `db::DatabaseMediator::FindChildFiles`, `str:Import Track`, `str:Import File`
- Import with beat grid supplied (from XML or device). `sym:DatabaseIF::importTrackWithBeat`, `DatabaseMediator::importBeatInfo`
- Remove from collection, moving rows to Trash (Trash box can be emptied or restored). `str:Remove from Collection`, `rb::middleware::db::rekordbox::{TrashContent,TrashPlaylist,TrashSongPlaylist}`, `EmptyTrashBoxService`, `RestorePlaylistService`, `RemoveTrashPlaylistService`, `sym:rekordboxDBController::emptyTrashBox`, `getAllTrashBoxTracks`
- Edit track metadata inline and in the info dialog: title, artist, album, album artist, genre, label, composer, lyricist, remixer, original artist, comment, rating, colour, key, BPM, year, release date, track no, disc no, subtitle (mix name), ISRC, DJ play count. `sym:rekordboxDBController::modifyTitle`, `modifyGenre`, `modifyLabel`, `modifyComposer`, `modifyLyricist`, `modifyRemixer`, `modifyOrgArtist`, `modifyComment`, `modifyRating`, `modifyColorId`, `modifyKeyName`, `modifyBPM`, `modifyReleaseYear`, `modifyReleaseDate`, `modifyTrackNo`, `modifyDiscNo`, `modifySubtitle`, `modifyDJPlayCount`
- Reset DJ play count(s); played-status colouring of tracks in current session; "Link played" status from CDJs. `str:Reset DJ Play Counts`, `sym:clearAllPlayedStatus`, `clearAllLinkPlayedStatus`, `str:Coloring of played tracks`, `db::PlayedSettingFile`
- Reload Tag(s): re-read file tags into DB. `str:Reload Tags`, `sym:DatabaseMediator::reloadTag`, `readTag`, `modifyReloadTag`
- Write tags/artwork back to file (TagLib) when editing artwork. `sym:DatabaseMediator::writeTagImage`, `TagLibParser.cpp`
- Artwork: add/replace/delete per track and per playlist; rescaled small/medium/large copies. `sym:addArtwork`, `deleteTrackArtwork`, `deleteListArtwork`, `modifyListArtwork`, `create_rescaled_image`, `create_small_arts_if_not_exists`, `str:*_s.jpg`, `str:*_m.jpg`, `str:*_l.jpg`
- Track-relation (track linking, "ContentLink"), video association (link a video file to an audio track). `sym:addTrackRelation`, `changeTrackRelation`, `DatabaseIF::getAssociatedVideo`, `deleteAssociate`, column `VideoAssociate`
- Hot cue auto-load flag per track; KUVO delivery control/comment per track ("public" flags). `sym:modifyHotCueAutoLoad`, `modifyDeliveryControl`, `modifyDeliveryComment`, `setAllKuvoPublic`
- Analysis lock per track (protect grid/cues from re-analysis). `sym:modifyAnalysisLock`, `changeAnalysisLock`
- Auto-gain values per track (`djmdMixerParam` gain/peak). `sym:addAutoGainInfo`, `getTrackAutoGainInfo`
- Active Censor ranges per track (in/out ms, parameters). `rb::middleware::db::rekordbox::ActiveCensor`, `sym:setActiveCensorInPoint`, `setActiveCensorOutPoint`
- Stem cue (stem mute state per cue). `rb::middleware::db::rekordbox::StemCueEntity` (fields ActiveStemCue, MuteBass/Drums/Inst/Vocal flags), `str:contentStemCue`, `str:rb_stem_cue_count`
- Monthly report: genre and play statistics aggregated from histories; content registration counts by year and month (Collection Filter by acquisition month). `rb::middleware::db::rekordbox::MonthlyReportService`, `str:SELECT strftime('%Y', created_at) ... GROUP BY year, month`, `sym:DatabaseIF::getContentsByYearAndMonth`
- Duplicate file detection and merge. `str:Display All Duplicated Files`, `str:Duplicated File Manager`, `DuplicatedFileDialog.cpp`, `DuplicatedFileMergeDialog.cpp`, `sym:isCollectionSongForDuplicateSongId`
- Convert file format and replace in library. `str:Convert`, `str:ConvertFinish`
- Convert memory cues to hot cues (batch). `str:Convert Memory Cues to Hot Cues`, `ConvertMemoryCueToHotCue.cpp`
- Streaming tracks (Beatport, Beatsource, SoundCloud, Tidal, Spotify, Apple Music) stored as `djmdContent` rows with `ServiceID != 0` and a service URI in `FolderPath`; Beatsource-to-Beatport migration rewrites them. `str:select FolderPath from djmdContent where rb_local_deleted = 0 and ServiceID != 0`, `db::DatabaseMediator::BsToBpMigrationManager`. Streaming bridges themselves belong to the streaming subsystem.

### 2.3 Playlists, folders and other trees

- Playlist tree with folders, arbitrary depth, manual ordering (`Seq`), rename, move, duplicate, delete, artwork, icon colour. `rb::middleware::db::rekordbox::Playlist`, `PlaylistIconColorEntity`, `sym:createNewList`, `moveList`, `modifyListName`, `deleteList`, `str:Duplicate the selected item`
- Playlist track ordering (`TrackNo`), renumbering after insert/remove/move, repair of broken order. `sym:renumberPlaylistTrack`, `fixPlaylistTracksOrder`, `repairPlaylistTrackOrder`, `str:CREATE TEMPORARY TABLE temp_table(__RENUMBER_COLUMN__ ...)`
- Duplicate-track prevention in a playlist (option). `str:Duplicate tracks cannot be added to this playlist.`
- Search playlists/folders by keyword; pin playlists; tree shortcuts. `rb::middleware::db::rekordbox::PlaylistSearchService`, `PlaylistSearchCondition`, `str:Pin playlist`, `str:Tree Shortcut:`
- Smart (intelligent) playlists: match all/any of N conditions, optional auto-update, operators include "is", "contains", "does not contain", "starts with", "ends with", "is in the range", "is in the last", "is not in the last" with day/month units; conditions over artist, album, album artist, original artist, genre, label, composer, remixer, BPM, key, rating, colour, MyTag, date added, release date, year, comments, file name, DJ play count, etc. Stored as XML in `djmdPlaylist.SmartList`. `SmartPlaylist::{Node,Condition,Parser,Writer}`, `SmartPlaylistComp`, `str:Match the following condition:`, `str:all of the`, `str:any of the`, `str:Update automatically`, `str:is in the last`, `agent:NODE CONDITION PropertyName ValueLeft LogicalOperator Operator ValueRight ValueUnit AutomaticUpdate`
- When a MyTag is deleted or re-IDed, smart playlist criteria referencing it are rewritten. `sym:DatabaseMediator::fixMyTagIdsInSmartlistCriteria`, `agent:modifyIntelligentPlaylistsByMyTagIDs`
- Smart playlist sort setting. `db::SmartListSortSetting`
- Histories: automatic per-day history playlists ("HISTORY yyyy-mm-dd") inside year/month folders, with "played after N seconds" threshold; manual edit/delete; import histories from devices. `rb::middleware::db::rekordbox::History`, `SongHistory`, `str:HISTORY %04d-%02d-%02d`, `str:The length of time before a track is added to Histories`, `DeviceHistory::ImportedHistoryManager`, `sym:importHistoryFromDevice`
- Tag List (single ordered working list), Request Catalog (max 200 tracks). `str:Tag List`, `str:Request Catalog (xxx/200 Tracks added)`, `rb::middleware::db::rekordbox::{SongTagList,SongRequestList}`
- Hot Cue Bank lists: folders of banks, each bank a list of up to 8 cue slots copied from tracks (cue data duplicated into `djmdSongHotCueBanklist`). `rb::middleware::db::rekordbox::{HotCueBanklist,SongHotCueBanklist,HotCueBanklistCue}`, `str:Set/Play Hot Cue Bank A`
- Sampler lists (sampler palette playlists) and Video lists. `rb::middleware::db::rekordbox::Sampler`, `SongSampler`, `sym:addNewSamplerList`, `addNewVideoList`
- Automix playlist (stored as an XML file, not in DB). `str:automixPlaylist6.xml`, `str:Save Automix playlist`
- Playlist palette (multiple playlists displayed side by side). `str:Show/Hide Playlist Palette`, `PlaylistPalette.cpp`
- Shared playlists (cloud, invitation-based, members, edit history; limits 100 playlists/account, 5 members, 1000 tracks). `rb::middleware::db::rekordbox::{SharedPlaylist,SharedPlaylistUser}`, `agent:shared_playlists max_playlist_count_per_account 100 max_member_count_per_playlist 5 max_track_count_per_playlist 1000`
- Cloud Export playlists and CloudDirectPlay filter playlists (separate trees mirrored to cloud). `CloudExportPlaylistEntity`, `CloudExportSongPlaylist`, `CloudFilterPlaylist`

### 2.4 MyTag, rating, colour, key

- MyTag: two-level tree (group columns under root, tags under groups), create/rename/move/delete, assign/unassign to tracks, MyTag filter panel; optionally mirrored into the comment field. `rb::middleware::db::rekordbox::{MyTag,SongMyTag}`, `browse::BrowseMyTagView`, `sym:putMyTagOnTrack`, `removeMyTagFromTrack`, `updateCommnetByMyTagUpdate`, `str:My Tag Configuration Window`
- Ratings 0 to 5 stars. `TrackListSorter::compareRate`, `db::TrackFilterData::setRatingFilter`
- Colour labels: 8 fixed colours with user-editable comment text. `rb::middleware::db::rekordbox::Color` (ColorCode, SortKey, Commnt), `str:0xFF007F` ... `str:0x660099`, `sym:modifyColorComment`
- Key table: textual scale names, created on demand; key display notation switch (classic/alphanumeric). `rb::middleware::db::rekordbox::Key` (ScaleName, Seq), `sym:getAndAddKeyID`, `DatabaseMediator::switchKeyString`
- Lookup tables (artist/album/genre/label/key) are get-or-create and orphan rows are garbage collected. `sym:getAndAddArtistID`, `str:select ID from djmdArtist where ... ID not in (select ArtistID ...)`

### 2.5 Browser: search, filter, sort, columns

- Keyword search over collection, current list, or all playlists, with collation rules contains/equals/startsWith/endsWith and OR search. `db::DbSearchEngine::{selectByKeyword,selectByKeywordOR,selectPlaylistsByKeyword}`, `db::CollationRule`, `str:Search for tracks in Collections`
- Category filter (Genre/Artist/Album columns, column view mode). `db::CategoryFilterData`, `str:Category Filter:`
- Track filter bar: BPM range, key (incl. harmonic matching), rating, colour, MyTag (AND/OR). `db::TrackFilterData`, `db::TrackListFilter::execANDFilter`, `db::AttributeFilter`
- Sort by any column with natural-order string compare; columns include title, artist, album, album artist, genre, label, composer, lyricist, remixer, original artist, mix name, BPM, key, rating, colour, comment, date added, date created, release date, release year, duration, bit rate, bit depth, sample rate, file type, file size, file name, file path, track no, disc no, play count, artwork, status, grouping, auto hot cue, public/KUVO. `db::TrackListSorter` (`compareAlbum` ... `compareTrackNo`, `natcmp`, `prepareStringCache`)
- Configurable category and sort menus (persisted in `djmdCategory`, `djmdSort`, `djmdMenuItems`; bit 0x01 of `Disable` hides an item, bit 0x02 is a second state (**inference**: sub-column selection)). `str:update djmdSort set Disable = (Disable | 0x02)`, `str:ERROR Mismatch Category Setting djmdCategory`
- Column set and widths persisted per view (setting file). `browse::BrowseSettingFile`, `sym:DatabaseIF::getRightColumnSetting`

### 2.6 Related tracks and track suggestion

- Related Tracks: user-defined criteria sets stored as a tree (`djmdRelatedTracks.Criteria` is JSON); criteria: same artist, BPM (± range or expression), key (matching keys), genre, label, composer, remixer, year (range/expression), date added, rating, colour, comment, MyTag, file format (audio/video), history, "matching" (playlist relation). Search source can be collection or a chosen playlist. "Turn the Related Tracks into a playlist". `db::RelatedTrackCriteria::{BPM,Comment,DateAdded,FileFormat,History,Key,MyTag,Rating,SameArtist,Year}`, `db::RelatedTrack*Search`, `db::RelatedTrackSearchSourceSelector`, `db::AsyncRelatedTrackSearch`, `str:Turn the "Related Tracks" into a playlist`
- Track Suggestion: ranks every track against the playing one, by type (similar year "Era", similar mood "Mood", association "Story"), with weights loaded from JSON (default or server-supplied). `db::TrackSuggestion`, `db::TrackSuggestion::WeightConfig::{ms_externalWeightEra,ms_externalWeightMood,ms_externalWeightStory}`, `str:Tracks with a close year of release.`, `str:Tracks with a similar mood.`
- Collection Radar / Streaming Radar: cloud-side embedding similarity (needs uploaded analysis), not available for >30 min tracks or USB-exported tracks. `sym:TrackSuggestion::searchCloud`, `RowDataTrack::getEmbeddingData`, `str:Collection Radar/Streaming Radar is unavailable for tracks with a time over 30 minutes.`
- "Recommend like" pairs (track A liked after track B) with like rate. `rb::middleware::db::rekordbox::RecommendLike` (ContentID1, ContentID2, LikeRate, DataCreatedHigh/Low), `RecommendLikeService`
- Personal cue trend file. `str:cue_personal_trend.json`, `sym:DatabaseIF::StartPersonalTrend`

### 2.7 Missing files and relocation

- Missing File Manager: list all missing tracks, relocate selected (file chooser), auto relocate by searching configured folders (Desktop, Music, Movies, user folders), delete selected. `MissingFileTable::{relocateSelectedFiles,deleteSelectedFiles}`, `str:Display All Missing Files`, `str:Auto Relocate Search Folders`, `str:Specified user folders`, `sym:DatabaseIF::getMissingTrack`, `rekordboxDBController::relocateTrack`, `relocateAnalysis`, `relocateArtwork`
- Path normalisation and bulk prefix replacement (drive letter/volume change). `sym:normalizeAllContentPath`, `replaceAllContentPath`, `replaceAllAnalysisPioneerFolder`, `replaceAllArtworkPioneerFolder`
- File-existence flag cached and refreshed. `sym:changeFileExistence`, `checkExistence`

### 2.8 Import and export

- rekordbox XML export ("Export Collection in xml format") and import ("Imported Library" node in Bridge, select XML in Preferences); optional beat grid export; import as track, playlist, or with beat grid. `db::DjPlaylistsWriter`, `db::DjPlaylistsAnalyzer`, `db::DjPlaylistsBridge`, `db::SimpleXmlReader`, `DetailrbXmlExportBeatInfoComponent`, `DetailrbXmlImportLibraryComponent`, `str:DJ_PLAYLISTS`, `str:bridgeExportBeatGrid`, `sym:DatabaseMediator::importRBXmlData`, `updateCollectionFromXml`, `rekordbox::ImportXmlTrackAlert`
- Playlist file import: m3u, m3u8, pls (file URIs converted to paths, multi-byte decoding). `db::PlayListFileParser::{parseM3U,parseM3U8,parsePLS,convertURItoFilepath}`, `PlayListFileParserMac.mm`, `str:Import Playlist`
- Playlist export to `.txt` (tab-separated list) and `.m3u8` "for music apps". `str:Export a playlist to a file (*.txt)`, `str:Export a playlist to a file for music apps (*.m3u8)`, `sym:DatabaseIF::exportPlaylist`
- iTunes / Music.app library bridge: read via Apple ITLibrary framework or the iTunes Library XML; browse its playlists (incl. smart playlists) in the tree; import tracks/playlists; "Convert iTunes Grouping to rekordbox Label"; overwrite option; warn when the configured XML differs from the one iTunes uses; sync selected playlists; read artwork from ITLibrary. `db::iTunesBridge`, `db::iTunesBridge::Pimpl::{analyzeMusicApp,getArtworkFromITLib}`, `db::iTunesAnalyser`, `ItunesSyncSetting`, `DetailiTunes{Convert,LibraryFile,LodingType,Overwrite}Component`, `str:iTunes/iTunes Music Library.xml`, `str:Convert iTunes "Grouping" to rekordbox "Label"`, `str:Unsupported Intelligent playlist.`, `str:iTunes library specified in Preferences is different from that used by iTunes.`
- Import library from rekordbox 5 (or earlier): reads DeviceSQL `datafile.edb` / `ExtData.edb` plus `masterPlaylists3.xml`, converts to `master.db`; first-run conversion wizard. `str:Import your library from ver. 5 (or earlier)`, `str:You'll need to convert the library you used with rekordbox ver. 5`, `str:datafile.edb`, `str:ExtData.edb`, `edb_dyn_*` exports, `V6IntroductionDialogWindow`, `str:fromV5/`
- Import from device (USB/SD library) to collection, including cue/history update; export to device is out of scope here (see export subsystem). `db::DatabaseMediator::importFromDeviceThread`, `sym:importTrackFromDevice`, `importDevicePlaylist`
- Import sampler packs (zip). `sym:importSamplerPack`, `res:GROOVE_CIRCUIT_FACTORY_SAMPLES.spp`

### 2.9 Backup, restore, library move

- "Backup Library": zip containing `master.db` (+WAL/SHM), setting files, `KeyMappings/`, `Sequences/`, optionally all analysis/artwork, with a `contents.xml` manifest carrying `CONTENT_NUM`, `BACKUP_RB_VER` and the original base directories. `db::BackupZipBuilder`, `db::LibraryBackupThread`, `db::PrepareBackupThread`, `str:contents.xml`, `str:CONTENT_NUM`, `str:BACKUP_RB_VER`, `str:KeyMappings/`, `str:Sequences/`, `str:rekordbox_bak`
- "Restore Library": unzip, relocate directory paths to the new machine, restore DB, report tracks that could not be restored (log in Documents), require restart. `db::LibraryRestoreThread`, `db::PrepareRestoreThread`, `db::UnZipFile`, `str:relocateDirectoryPathArray`, `str:One or more tracks were not restored.`, `sym:restoreDBfromBackupFilesVer6`
- Restore from local automatic backup. `db::RestoreFromLocalBackupThread`, `sym:DatabaseMediator::restoreFromLocalBackup`
- Device library backup to cloud and restore (with missing-track dialog). `CloudDeviceLibraryBackup`, `BackedupDeviceLibDialog`, `DeviceLibRestoreMissingTrackDialog`
- Move library (DB plus PIONEER folder) to another drive with free-space check. `db::DatabaseMediator::PioneerFolderFileSizeCalculator`, `str:Insufficient space left on device`, `str:Master Database is moved successfully.`

### 2.10 Cloud library sync (AppSync) DB side

- When Cloud Library Sync is on, the desktop app does not write `master.db` directly; it talks REST to the local `rekordboxAgent` (Electron, Sequelize on SQLCipher) on `http://127.0.0.1:30001/api/v1` with a bearer token. `db::AppSyncDBController`, `db::CloudAgentAPI::{Agent,CrudIF,ListCrudIF,SongListCrudIF,LocalSync,Transaction}`, `str:http://127.0.0.1:30001/api/v1`, `str:Authorization: Bearer `, `agent:server.port 30001`
- Agent modes: standalone, sync, trial_sync, trial_export_sync; sync condition enabled/disabled; sync control run/pause; shared-playlist mode. `agent:config.json app.mode.options`, `str:/agent/mode`, `str:/agent/syncCondition`
- Agent launched as a LaunchAgent with `--db-path`, `--port`, `--analysis-data-root-path`, `--settings-root-path`, `--lang-path`, `--lang`, `--app_ver`. `str:Library/LaunchAgents`, `str:--db-path=`
- Endpoints: `/agent/{login,token,env,mode,quit,messages,showNotification,syncCondition,sharedPlaylists/...}`, `/data/<table>/...` CRUD, `/data/<table>/<id>/file` and `/file/request`, `/renumber`, `/<id>/move`, `/data/settingFiles`, `/localSync/{prepare,close,...}`, long-poll for change events. `str:/data/__TABLE_NAME__/%lu/file/request`, `db::AppSyncDBController::LongPoll`, `agent:routes/api/...`
- Change tracking columns on every synced row (`rb_data_status`, `rb_local_deleted`, `rb_local_synced`, `usn`, `rb_local_usn`, `UUID`, `created_at`, `updated_at`), soft delete when a row has been synced and hard delete otherwise, a monotonic local update counter in `agentRegistry('localUpdateCount')`. `str:update __TABLE__ set rb_local_deleted = 1, rb_data_status = case when rb_data_status != 0 then 258 else 2 end ...`, `str:select int_1 as rb_local_usn from agentRegistry where registry_id = 'localUpdateCount'`
- File sync records for audio, artwork and settings (`contentFile`, `imageFile`, `settingFile`) with hash, size, dirty flags, in-sync hash/USN. `rb::middleware::db::rekordbox::SettingFile` (FIELD_HASH, FIELD_INSYNC_HASH, ...), `agent:contentFile imageFile settingFile`
- Cloud storage services (Dropbox, Google Drive) for audio files; auto upload per playlist. `RowDataTrack::isDropboxTrack`, `isGoogleDriveTrack`, `str:https://notify.dropboxapi.com/2/files/list_folder/longpoll`. (Transport belongs to the cloud subsystem.)
- Mobile library sync (rekordbox for iOS/Android) via the same mechanism. `CloudMobileLibrarySyncFile.cpp`, `str:rekordbox/Imported from Mobile`

### 2.11 DB server for linked players (touch point)

- `PSvDBMain`/`PSvDBServer` answer browse requests from CDJs/XDJs over Pro DJ Link (rekordbox Link Export mode) by querying `master.db` through `PSvMasterDBIF` (direct) or `PSvAppSyncDBIF` (agent mode): root menu, category lists (artist, album, genre, BPM, key, rating, colour, year, label, bitrate, length, history, MyTag, filename, matching), track lists, cue/hot-cue-bank data, artwork, waveforms, filter conditions. These consume this subsystem's queries; the protocol is owned by the prolink subsystem. `PSvDBMain::On*ListCmd`, `PSvMasterDBIF::getTrack_*`, `str:CMD_SET_FILTER_CONDITION_MYTAG`

### 2.12 Out of scope here (owned elsewhere, noted for interfaces)

- Device export (`export.pdb`, `exportExt.pdb`, OneLibrary `exportLibrary.db`, `ANLZ*.DAT/.EXT/.2EX`, `MYSETTING*.DAT`): `db::DevSqliteDBController`, `DatabaseMediator::export_*`, `deviceSqliteDbConversion`. The OneLibrary device schema strings (`titleForSearch`, `bpmx100`, `isKuvoDeliverStatusOn`, `hotCueBankList_cue`, ...) belong to that spec.
- Analysis files themselves (`AnalysisDataPath` points at `USBANLZ/<hash>/ANLZ0000.DAT`), streaming service bridges, lighting DB.

---

## 3. Data formats and schemas

### 3.1 master.db container

- SQLite 3 with SQLCipher page encryption. Only `sqlite3_key` is imported (no `cipher_*` pragmas found in strings), so SQLCipher 4 defaults are assumed (**inference**, consistent with pyrekordbox which documents "SQLCipher4"). The shipped template and `product.db` both have non-SQLite first bytes, confirming whole-file encryption.
- Key: one fixed key shared by all installs (not per-machine or per-licence, per pyrekordbox docs). In rekordbox 7 the agent's `options.json` (`rekordboxAgent/storage/options.json`) carries the key in an obfuscated form that is decoded with a fixed cipher (the agent bundles `egoroof-blowfish`). The native app's key source is `rb::middleware::db::rekordbox::MasterDbKey`. Not recorded here; see open question Q1.
- `product.db` has its own password from `rb::middleware::db::product::ProductDB::getDbPassword`.
- Timestamps: `created_at`/`updated_at` stored as text `YYYY-MM-DD hh:mm:ss.SSS +00:00` (UTC). `str:%04d-%02d-%02d %02d:%02d:%02d.%03d +00:00`, `agent:YYYY-MM-DD hh:mm:ss.SSS Z`
- IDs: `ID` columns are `VARCHAR(255)` holding decimal integers; new IDs are random unused integers (`generateUnusedID`), plus a `UUID` string column for sync identity. A `uuidIDMap(TableName, TargetUUID, CurrentID)` table resolves ID collisions with the server. `agent:model_util generateRandomIDSet generateUnusedID`, `agent:uuidIDMap`
- Tree tables use `ParentID` = `'root'` for top level and `Seq` for sibling order. `str:where ParentID = 'root'`

### 3.2 Sync bookkeeping columns (on every synced djmd* table)

| Column | Type | Meaning |
|---|---|---|
| `UUID` | VARCHAR(255) | Stable cross-device identity |
| `rb_data_status` | INTEGER | 0 = created locally, never synced; 2 = deleted before sync; 257 (0x101) = modified after sync; 258 (0x102) = deleted after sync (**inference** from update/delete SQL; agent names `IN_SYNC`, `SYNC_MODIFIED`, `LOCAL_DELETED`, `SYNC_DELETED`, `IN_SYNC_DELETED`) |
| `rb_local_data_status` | INTEGER | Local-only variant of the above |
| `rb_local_deleted` | TINYINT(1) | Soft-delete flag; every query filters `rb_local_deleted = 0` |
| `rb_local_synced` | TINYINT(1) | Row has been synced at least once |
| `usn` | BIGINT | Server update sequence number |
| `rb_local_usn` | BIGINT | Local update sequence number (from `agentRegistry.localUpdateCount`) |
| `created_at`, `updated_at` | DATETIME NOT NULL | UTC timestamps |

Soft vs hard delete rule: rows with `rb_data_status = 0 and rb_local_synced = 0` are physically deleted; others are marked deleted. `str:delete from __TABLE__ where rb_local_deleted = 0 and rb_data_status = 0 and rb_local_synced = 0 ...`

### 3.3 Tables (columns from SQL strings, entity FIELD_ constants and agent models)

Common bookkeeping columns from 3.2 are omitted below.

| Table | Columns | Notes |
|---|---|---|
| `djmdContent` | ID, FolderPath, FileNameL, FileNameS, Title, ArtistID, AlbumID, GenreID, BPM (int, BPM×100), Length (s), TrackNo, BitRate, BitDepth, Commnt, FileType, Rating, ReleaseYear, RemixerID, LabelID, OrgArtistID, KeyID, StockDate, ColorID, DJPlayCount, ImagePath, MasterDBID, MasterSongID, AnalysisDataPath, SearchStr, FileSize, DiscNo, ComposerID, Subtitle, SampleRate, DisableQuantize, Analysed, ReleaseDate, DateCreated, ContentLink, Tag, ModifiedByRBM, HotCueAutoLoad, DeliveryControl, DeliveryComment, CueUpdated, AnalysisUpdated, TrackInfoUpdated, Lyricist, ISRC, SamplerTrackInfo, SamplerPlayOffset, SamplerGain (float), VideoAssociate, LyricStatus, ServiceID, OrgFolderPath, Reserved1..4, ExtInfo, rb_file_id, DeviceID, rb_LocalFolderPath, SrcID, SrcTitle, SrcArtistName, SrcAlbumName, SrcLength | Full list mirrored by `djmdTrashContent` DDL `str:CREATE TABLE IF NOT EXISTS djmdTrashContent`. BPM×100 per prior art. `ExtInfo` holds JSON (e.g. Apple Music extras) **(inference)** |
| `djmdArtist` | ID, Name, SearchStr | |
| `djmdAlbum` | ID, Name, AlbumArtistID, ImagePath, Compilation, SearchStr | |
| `djmdGenre`, `djmdLabel` | ID, Name | |
| `djmdKey` | ID, ScaleName, Seq | |
| `djmdColor` | ID, ColorCode, SortKey, Commnt | 8 rows |
| `djmdCue` | ID, ContentID, InMsec, InFrame, InMpegFrame, InMpegAbs, OutMsec, OutFrame, OutMpegFrame, OutMpegAbs, Kind, Color, ColorTableIndex, ActiveLoop, Comment, BeatLoopSize, CueMicrosec, InPointSeekInfo, OutPointSeekInfo, ContentUUID | Index on (ContentID, rb_local_deleted). Kind 0 = memory cue, 1..8 = hot cue A..H (prior art). Local-only in agent; synced via `contentCue` |
| `contentCue` | ID, ContentID, Cues (JSON array), rb_cue_count | Sync carrier of all cues of a track |
| `contentStemCue` | ContentID, StemCues (JSON), rb_stem_cue_count | **(inference)** same pattern |
| `djmdActiveCensor` | ID, ContentID, InMsec, OutMsec, Info, ParameterList, ContentUUID | synced via `contentActiveCensor(ContentID, ActiveCensors JSON, rb_activecensor_count)` |
| `djmdMixerParam` | ID, ContentID, GainHigh, GainLow, PeakHigh, PeakLow | auto-gain as split 32-bit halves **(inference)** |
| `djmdPlaylist` | ID, Seq, Name, ImagePath, Attribute, ParentID, SmartList (XML text) | Attribute: 0 playlist, 1 folder, 4 smart (prior art); bit 0x80 queried with cloud sync (**inference**: sync-selected); values 2 and 8 also tested (`str:Attribute = 2`, `str:Attribute = 8`) |
| `djmdSongPlaylist` | ID, PlaylistID, ContentID, TrackNo | |
| `djmdPlaylistIconColor` | ID, ColorID | per playlist icon colour |
| `djmdHistory` | ID, Seq, Name, Attribute, ParentID, DateCreated | |
| `djmdSongHistory` | ID, HistoryID, ContentID, TrackNo | |
| `djmdSongTagList` | ID, ContentID, TrackNo | synced through `cloudAgentRegistry` JSON blob |
| `djmdSongRequestList` | ID, ContentID, TrackNo | same |
| `djmdMyTag` | ID, Seq, Name, Attribute, ParentID | |
| `djmdSongMyTag` | ID, MyTagID, ContentID, TrackNo | |
| `djmdHotCueBanklist` | ID, Seq, Name, ImagePath, Attribute, ParentID | |
| `djmdSongHotCueBanklist` | ID, HotCueBanklistID, ContentID, TrackNo, CueID, InMsec, InFrame, InMpegFrame, InMpegAbs, OutMsec, OutFrame, OutMpegFrame, OutMpegAbs, Color, ColorTableIndex, ActiveLoop, Comment, BeatLoopSize, CueMicrosec, InPointSeekInfo, OutPointSeekInfo, HotCueBanklistUUID | synced via `hotCueBanklistCue(HotCueBanklistID, Cues JSON, rb_cue_count)` |
| `djmdSampler` | ID, Seq, Name, Attribute, ParentID | |
| `djmdSongSampler` | ID, SamplerID, ContentID, TrackNo | |
| `djmdRelatedTracks` | ID, Seq, Name, Attribute, ParentID, Criteria (JSON) | |
| `djmdSongRelatedTracks` | ID, RelatedTracksID, ContentID, TrackNo | |
| `djmdRecommendLike` | ID, ContentID1, ContentID2, LikeRate, DataCreatedH, DataCreatedL | |
| `djmdCategory` | ID, MenuItemID, Seq, Disable, InfoOrder | |
| `djmdSort` | ID, MenuItemID, Seq, Disable | |
| `djmdMenuItems` | ID, Class, Name | |
| `djmdProperty` | DBID, DBVersion, BaseDBDrive, CurrentDBDrive, DeviceID, Reserved1..5 | local only |
| `djmdDevice` | ID, MasterDBID, Name | |
| `djmdCloudProperty` | ID, Reserved1..5 | identifiers `SyncService`, CDP filter version |
| `djmdCloudExportPlaylist` | ID, Seq, Name, ImagePath, Attribute, ParentID, SmartList | |
| `djmdCloudExportSongPlaylist` | ID, CloudExportPlaylistID, ContentID, TrackNo | |
| `djmdCloudFilterPlaylist` | ID, PlaylistUUID, Seq, ParentID | |
| `djmdSharedPlaylist` | ID, data_selection, edited_at, int_1/2, str_1/2, text_1/2 | local only |
| `djmdSharedPlaylistUser` | ID, member_type, member_id, status, invitation_expires_at, invited_at, joined_at, ... | local only |
| `djmdTrashContent`, `djmdTrashPlaylist`, `djmdTrashSongPlaylist` | as content/playlist/songplaylist | DDL in binary |
| `contentFile` | ID, ContentID, Path, Hash, Size, rb_local_path, rb_insync_hash, rb_insync_local_usn, rb_file_hash_dirty, rb_local_file_status, rb_in_progress, rb_process_type, rb_temp_path, rb_priority, rb_file_size_dirty | |
| `imageFile` | ID, TableName, TargetUUID, TargetID, Path, Hash, Size, + same rb_* file columns | |
| `settingFile` | ID, Path, Hash, Size, rb_local_path, rb_insync_hash, rb_insync_local_usn, rb_file_hash_dirty, rb_file_size_dirty | |
| `agentRegistry` | registry_id, id_1, id_2, int_1, int_2, str_1, str_2, date_1, date_2, text_1, text_2 | key/value; `localUpdateCount` etc. local only |
| `cloudAgentRegistry` | ID, int_1/2, str_1/2, date_1/2, text_1/2 | synced; carries tag list and request list JSON (`VALUE_SONG_TAG_LIST`, `VALUE_SONG_REQUEST_LIST`) |
| `uuidIDMap` | TableName, TargetUUID, CurrentID | |

`djmdLeftBuf` and `djmdBPMRange`, `djmdTrackSort`, `djmdImage` appear in strings only as names in the PSv/dsql DB-server layer; they are probably in-memory or device-side tables (open question Q6).

### 3.4 Smart playlist XML (djmdPlaylist.SmartList)

Root `<NODE>` with attributes including `Id`, `LogicalOperator` (all/any) and `AutomaticUpdate`; children `<CONDITION PropertyName=... Operator=... ValueUnit=... ValueLeft=... ValueRight=.../>`. Property name tokens seen: `myTag`, `mixName`, `remixedBy`, `originalArtist`, `producer`, `albumArtist`, `counter`, `grouping`, `stockDate`, `dateReleased` (plus others shared with UI). `str:LogicalOperator`, `str:AutomaticUpdate`, `str:ValueUnit`, `agent:NODE CONDITION PropertyName ValueLeft`. Exact enumerations: pyrekordbox `smartlist` module (MIT) has a full property/operator table; to be cross-checked (Q3).

### 3.5 Related-tracks criteria JSON

`djmdRelatedTracks.Criteria` is JSON with a version and one object per criterion (SameArtist, BPM, Key, Genre, Label, Composer, Remixer, Year, DateAdded, Rating, Color, Comment, MyTag, FileFormat, History, Matching), each with an enable flag and parameters (range or expression). `db::RelatedTrackCriteria::{toJSON,fromJSON,appendVersion}`. Key names: Q4.

### 3.6 rekordbox XML (DJ_PLAYLISTS)

Publicly documented by Pioneer ("rekordbox XML format list"). Elements observed in the binary: `DJ_PLAYLISTS(Version)`, `PRODUCT(Name, Version, Company="AlphaTheta")`, `COLLECTION(Entries)`, `TRACK` (TrackID, Name, Artist, Composer, Album, Grouping, Genre, Kind, Size, TotalTime, DiscNumber, TrackNumber, Year, AverageBpm, DateModified, DateAdded, BitRate, SampleRate, Comments, PlayCount, LastPlayed, Rating, Location (file:// URI, percent-encoded with the reserved set `!#$()*+,-./:;=?@_~` left unescaped), Remixer, Tonality, Label, Mix, Colour), `TEMPO(Inizio, Bpm, Metro, Battito)`, `POSITION_MARK(Name, Type, Start, End, Num, Red, Green, Blue)`, `PLAYLISTS/NODE(Type, Name, Count | Entries, KeyType)` with `TRACK(Key)`. Colours written as `0xRRGGBB` from the eight label colours. `db::DjPlaylistsWriter::{addProductNode,addCollectionNode,addTrackNode,addTempoNode,addPositionMarkNode,addPlaylistsNode,addPlaylistNode,locationToURI}`, `str:Inizio`, `str:Battito`, `str:KeyType`. Import uses a streaming reader that handles UTF-8 and UTF-16 (`db::SimpleXmlReader::findStringUTF16`).

### 3.7 Other files

- `masterPlaylists6.xml` (playlist sync config for Bridge/iTunes, **inference**), `automixPlaylist6.xml`, legacy `masterPlaylists3.xml`, `smartlist.xml`, `automixPlaylist.xml`, `rbPhotoLibrary.xml`. `str:` same names.
- Settings that the backup carries: `rekordbox3.settings`, `PlaySettings.xml`, `FxUnitSettings.xml`, `PadFxSettings.xml`, `SamplerSettings1.xml`, `VideoSettings.xml`, `rekordboxEvent.xml`, `RBFLTR.DAT`, `DEVSETTING.DAT`, `DJMMYSETTING.DAT`, `BeatportSetting.txt`, `SoundCloudSetting.txt`, `KeyMappings/*.midi.csv`, `*.mappings`. `str:` near `rbPhotoLibrary.xml`.
- Backup archive: zip with `contents.xml` manifest (CONTENT_NUM, BACKUP_RB_VER, original directories such as userMusicDirectory), DB files, setting files, optional analysis/artwork. Restore log written to Documents. Format detail: Q8.
- Artwork store: `<PIONEER>/Artwork/<..>/artwork.jpg` with `_s`, `_m` (and `_l`) rescaled variants; `djmdContent.ImagePath` relative to DB root. `str:/Artwork/`, `str:/artwork.jpg`, `sym:calcMasterArtworkPath`, `calcImageRelPath`
- Analysis store: `<PIONEER>/USBANLZ/<dir>/<id>/ANLZ0000.DAT|.EXT|.2EX` referenced by `AnalysisDataPath` (format owned by analysis spec).
- rekordbox 5 legacy: DeviceSQL databases `datafile.edb`, `ExtData.edb` (+ `.backup`, `.backup2`, `.backup3`), page size 4096 (`str:PGA=SINGLE,PAGESZ=4096,NBUFS=6144,...`); `LocalData.edb`/`LocalDataExt.edb`.
- `ClsPlaylistDisplaySetting.json` (`hide_playlists`), `playlist_bank_set.json`, `cue_personal_trend.json`.
- iTunes: `~/Music/iTunes/iTunes Music Library.xml` or `iTunes Library.xml` (Apple plist XML), or ITLibrary framework.

---

## 4. Internal architecture as observed

Source tree (from build paths in strings): `Source/Middleware/DB/` with `main/` (DatabaseMediator, DbmBackupAndRestore, DbmSharedDB, DbmImport, DbmExport, DbmModify, DbmMoveAndSelectLibrary, DbmCloudDB, RelatedTracks/), `rekordbox/` (dao/, repository/, entity/, service/, di/, factory/, domain_repository/, dsql_wrapper_ext.cpp), `db_core/` (dao/sqlite_dao.h, log/, type/), `product/`, `Server/DBServer/` (dsql/, Generated), `SmartList/`, `PlayListFile/`, `Bridge/Streaming/`, `Cue/`, `Listener/`, `Tools/`.

Layering (top to bottom):

1. **UI** (`browse::*`, `MissingFileTable`, `SmartPlaylistComp`, preference pages) calls
2. **`db::DatabaseIF`** (facade, ~380 methods) which forwards to
3. **`db::DatabaseMediator`** (~1200 symbols): orchestration, threads, progress windows, file operations, import/export, backup/restore, cloud hooks, notifications (`notifyDBUpdated`, `notifyTrackListModified`, `notifyToDBServer`). Owns worker threads: `LibraryWorkerThread`, `AnalyzeCollectionThread`, `DbBackGroundProcess`, `CollectionUpdater`, `DBStatusUpdater`, `ImportManager`, `ImportHistoryManager`, `TrackModifyManager`, `RemoveListTrackManager`, `SharedDBMergeManager`, `BsToBpMigrationManager`, all deriving from `db::DbBaseThreadWithProgress` or similar.
4. **`db::IRekordboxDB`** interface with two implementations:
   - `db::rekordboxDBController` (~1270 symbols): direct SQL against `master.db`, keeps an in-memory track cache (`getCashedData`, hash maps by file path, analysis path hash: `recreateTrackHashMap`, `getTracklistIndexFromFilePathHash`) and a cue cache (`addCueInfoToCache`). Startup via `rekordboxDBController::InitializeManager`.
   - `db::AppSyncDBController` (~1100 symbols): same operations expressed as REST calls to rekordboxAgent (`db::CloudAgentAPI::*IF` per table, `AppSyncDBJob` queue, `LongPoll` thread for change events, `convertToContent`/`convertToRowDataTrack`).
   - `db::DevSqliteDBController`: device-side SQLite (export subsystem).
5. **New-style repository layer** `rb::middleware::db::rekordbox::*` (DDD style, used increasingly in v6/v7): entities with `FIELD_*` names, `TABLE_NAME`, `ToInsertSets/ToUpdateSets/ToJson`; `DaoBase<T>` (SelectAll/SelectById/Insert/Update/DeleteById/DeleteItems); `RepositoryBase`, `SongListRepositoryBase`, `SongTrackRepositoryBase`; `RepositoryFactory` creating ~40 repositories; `DomainRepositoryDIContainer`; services (`EmptyTrashBoxService`, `RemovePlaylistService`, `PlaylistSearchService`, `MonthlyReportService`, `RecommendLikeService`, `DataCreatedTime`); domain events (`PlaylistIconColorUpdatedEvent`, `StemCueUpdatedEvent`).
6. **Storage primitives**: `rb::middleware::db::SqLiteDao<T>` (Select, SelectOne, SelectCount, ExecuteQuery) on top of SQLiteCpp (`SQLite::Database`, `SQLite::Statement`; MIT licensed upstream) and the older `SQLiteDB` wrapper (open, createTable, addColumns, backup, integrityCheck, transactionStart/Finish). Transactions use `begin deferred|immediate|exclusive`. Logging: `DbLogger`, `PerformanceLog`, `Stopwatch`, `TraceLog`.
7. **Legacy dsql layer**: `dsql_*.cpp`, `dsql*.cpp`, `DsqlWrapper.cpp`, `edb_dyn_api_Wrapper.cpp`; `db::DsqlRow/DsqlRowSet/DsqlInt32/DsqlNvarchar`. A DeviceSQL-compatible API retained for (a) reading rekordbox 5 `.edb` files, (b) the `PSvDBMain` DB server that answers Pro DJ Link browse commands, now backed by SQLite through `PSvMasterDBIF`/`PSvAppSyncDBIF` (`RbDBIndex` handle). `edb_streamd` helper binary ships alongside (**inference**: legacy DeviceSQL stream daemon).

Caches: track list cache (`DatabaseIF::isListTrackCached`, `notifyTrackListCacheSettingChanged`), `ArtworkCache` singleton with async loader thread and timer (`ArtworkCache::Pimpl::{run_file,run_url,run_streaming,timerCallback}`, lookahead prefetch, sources: file tags, URIs, network), `browse::ArtworkStatus`, string cache for sorting (`TrackListSorter::prepareStringCache`), PSvDBMain list-command cache.

Threads: UI message thread; DB worker threads above; AppSync long-poll thread; artwork loader thread; related-track `AsyncRelatedTrackSearch::SearchJob` (thread pool); iTunes analysis thread (`iTunesAnalyser::run`); XML analysis/writer threads (`DjPlaylistsAnalyzer::run`, `DjPlaylistsWriter::run`); backup/restore/move threads with progress windows.

Change notification: `db::DatabaseListener`, `db::EventDispatcher`, `AscyncUpdateMessage`, `DBCommon*Message`, `PSvDBMain::Deliver*Update` to linked players, `AppSync` agent messages (`data.db.item.updated`, `data.db.item.id_changed`).

---

## 5. Prior art

| Project | Licence | Relevance | Notes |
|---|---|---|---|
| pyrekordbox (dylanljones) https://github.com/dylanljones/pyrekordbox, docs https://pyrekordbox.readthedocs.io | MIT | master.db (rb6/7) schema and SQLAlchemy models, key handling, smart list XML parser/writer, rekordbox XML read/write, ANLZ, MySetting | Best single reference for djmd* semantics (Attribute values, Cue Kind, FileType codes, smartlist enums). Docs list 29 djmd tables and state SQLCipher 4. Its docs leave `rb_data_status` as "unknown": our 3.2 decoding is new |
| rbox (Rust) https://docs.rs/rbox | MIT (stated for 0.1.1; check current) | Rust ORM for master.db, ANLZ, XML; `master-db` feature needs SQLCipher | Closest Rust prior art; reuse ideas, verify licence per version before borrowing code |
| rekordcrate (Holzhaus) https://github.com/Holzhaus/rekordcrate | MPL-2.0 | PDB, ANLZ, `*SETTING.DAT` | Export side; file-level MPL so can be a dependency |
| crate-digger / Deep Symmetry docs https://djl-analysis.deepsymmetry.org | EPL-1.0 (code), docs freely readable | PDB/ANLZ Kaitai structs, DB server protocol (dbserver) | Relevant to the PSvDBMain touch point |
| Mixxx rekordbox feature https://github.com/mixxxdj/mixxx | GPL-2.0-or-later | Reads device PDB/ANLZ | Read for behaviour only; do not copy code |
| rekordbox XML format list (Pioneer DJ/AlphaTheta, rekordbox.com developer/support pages) | Public documentation | Element/attribute definitions for DJ_PLAYLISTS | Authoritative for 3.6 |
| SQLiteCpp https://github.com/SRombauts/SQLiteCpp | MIT | Wrapper rekordbox uses | Confirms API shape only |
| Apple iTunes Library XML (plist) | Public format | iTunes import | Well documented plist dictionary of Tracks/Playlists |

Still unknown publicly: exact SQLCipher parameters for v7 (assumed defaults), agent REST payload shapes, `rb_data_status` full state machine, `contentCue.Cues` JSON schema, `djmdRelatedTracks.Criteria` JSON schema, backup zip `contents.xml` schema, track suggestion scoring formula and weights, DeviceSQL `.edb` file format (rekordbox 5 migration).

---

## 6. Open questions for Ghidra

| # | Symbol to inspect | Answers |
|---|---|---|
| Q1 | `rb::middleware::db::rekordbox::MasterDbKey::MasterDbKey`, `SQLiteDB::open`, `rb::middleware::db::product::ProductDB::getDbPassword` | Where the master.db and product.db keys come from (constant, derived, options.json), and whether any `PRAGMA cipher_*` is issued. Record derivation only, never the key |
| Q2 | `db::rekordboxDBController::InitializeManager::*`, `rekordboxDBController::checkTable`, `initExtendedDB` | Full schema migration steps and `djmdProperty.DBVersion` values per version |
| Q3 | `SmartPlaylist::Condition::createCondition`, `getTargetList`, `getRangeUnitList`, `SmartPlaylist::Node::parse`, `SmartPlaylist::Writer::save` | Full property/operator/unit enumerations and XML attribute set |
| Q4 | `db::RelatedTrackCriteria::toJSON`, `RelatedTrackCriteria::*::toJSON`, `appendVersion` | Criteria JSON key names, defaults, versioning |
| Q5 | `db::TrackSuggestion::executeScoreCalculation`, `WeightConfig::getDefaultWeight`, `SearchAndScores::calculateScores` | Suggestion scoring formula and default weights |
| Q6 | `PSvDBMain::OnListBuffCmd`, `dsql_insert_leftbuf` callers, `djmdLeftBuf` users | Whether `djmdLeftBuf`, `djmdBPMRange`, `djmdTrackSort`, `djmdImage` are real tables or temp/in-memory |
| Q7 | `db::DatabaseMediator::readTag`, `writeTagImage`, `TagLibParser` globals | Which tag frames are read for each field (ID3 TKEY, TBPM, COMM, POPM rating scale, MP4 atoms, Vorbis comments), and what is written back |
| Q8 | `db::DatabaseMediator::libraryBackupTo1..5`, `prepareBackupTo1..9`, `libraryRestoreFrom1..8`, `initBackupInfoFile` | Backup zip layout and `contents.xml` schema |
| Q9 | `db::DjPlaylistsWriter::addTrackNode`, `addPositionMarkNode`, `db::DjPlaylistsAnalyzer::analyzeTrack`, `getTrackCueInfo` | Exact XML number formats (BPM decimals, Start precision), memory vs hot cue mapping, Colour mapping, how unknown attributes are ignored |
| Q10 | `db::iTunesAnalyser::analyzeTrack`, `analyzePlaylist`, `iTunesBridge::Pimpl::analyzeMusicApp` | Fields mapped from iTunes, smart playlist handling, Grouping to Label |
| Q11 | `db::PlayListFileParser::parseM3U`, `parsePLS`, `fromMultiByte`; `DatabaseIF::exportPlaylist` | Encoding detection, relative path resolution, exact `.txt` export columns |
| Q12 | `db::AppSyncDBController::*` (e.g. `addMultiTracksToPlaylist`), `CloudAgentAPI::CrudIF::insert` | REST payloads and status codes for agent mode |
| Q13 | `rb::middleware::db::rekordbox::LibrarySync::{NewStatus,ToUpdateStatus,ToDeleteStatus,ReflectStatus}` | Confirms the `rb_data_status` state machine |
| Q14 | `db::rekordboxDBController::relocateTrack`, `MissingFileTable::relocateSelectedFiles`, auto-relocate worker | Matching rule used by Auto Relocate (file name only, size, duration?) |
| Q15 | `db::DbSearchEngine::selectByKeyword`, `db::CollationRule::contains` | Which columns are searched, normalisation (case, width, diacritics via `SearchStr`) |
| Q16 | `db::TrackListSorter::natcmp`, `compareJuceString` | Sort collation and natural ordering rules |
| Q17 | `edb_dyn_PageManagerOpen`, `DatabaseMediator::execDbConversion` | rekordbox 5 `.edb` reading path and field mapping into djmd tables |
| Q18 | `ArtworkCache::Pimpl::addImageToCache`, `DatabaseMediator::create_rescaled_image` | Artwork sizes, JPEG quality, directory naming |
| Q19 | `rekordboxDBController::addTrack`, `insertTrack`, `DatabaseMediator::importTrack` | Defaults set on import (StockDate, DateCreated, SearchStr, FileType mapping, MasterDBID/MasterSongID) |
| Q20 | `MaintenanceRepository::Vacuum` | Vacuum threshold value |

---

## 7. Rust plan

### 7.1 Crates and modules

`crates/cuepoint-library` (existing skeleton) becomes the home of this subsystem:

```
cuepoint-library/
  src/
    lib.rs
    db/            # connection, key provider trait, pragmas, migrations, transactions
      schema.rs    # table/column definitions, generated row structs
      migrate.rs   # create-if-missing, add-column, index creation, DBVersion
      sync_cols.rs # rb_data_status / usn bookkeeping, soft vs hard delete
      ids.rs       # random unused ID allocation, UUID handling, uuidIDMap
    model/         # Track, Artist, Album, Playlist(tree), History, MyTag, Cue, HotCueBank, ...
    repo/          # one repository per aggregate, mirroring RepositoryFactory
    service/       # trash, playlist search, monthly report, recommend-like, renumbering
    smartlist/     # XML model, parser, writer, evaluator (SQL or in-memory)
    query/         # keyword search, category filter, attribute filter, sorter (natural order)
    related/       # related-track criteria JSON, searchers, track suggestion scoring
    tags/          # tag read/write facade
    artwork/       # artwork store paths, rescaling, async cache
    io/
      rbxml.rs     # DJ_PLAYLISTS read/write
      playlist_files.rs # m3u, m3u8, pls, txt export
      itunes.rs    # iTunes/Music XML plist import
      v5.rs        # rekordbox 5 .edb import (late, behind feature)
    backup/        # zip backup/restore, auto-backup rotation, move library
    missing.rs     # missing file scan, relocate, auto relocate
    agent/         # optional: sync-mode client (cloud subsystem may own)
```

A separate tiny crate `cuepoint-sqlcipher` (or a feature in `cuepoint-library`) isolates the encryption backend so the rest only sees a `Connection` trait.

### 7.2 Dependencies

- **SQLite + SQLCipher (the one hard C dependency).** Pure-Rust options: `limbo`/Turso (pure-Rust SQLite rewrite) does not implement SQLCipher's on-disk format; writing our own SQLCipher pager is possible (AES-256-CBC per page, HMAC-SHA512, PBKDF2-HMAC-SHA512 64000 iterations, 16-byte salt in page 1, reserve bytes per page under SQLCipher 4 defaults) using `aes`, `cbc`, `hmac`, `sha2`, `pbkdf2` crates, but we would still need a SQL engine. Recommended path:
  1. Ship v1 with `rusqlite` + `libsqlite3-sys` feature `bundled-sqlcipher-vendored-openssl` (C, statically built, no system libs). Keep all SQL behind our `db` module.
  2. Separately implement a pure-Rust SQLCipher 4 page codec (decrypt to plain SQLite image and re-encrypt) for tooling, tests and import/export of rekordbox DBs without C. If a pure-Rust SQLite engine with a pluggable pager matures, switch the runtime to it.
  3. Our own native library format may be plain SQLite (or our own encryption); SQLCipher is only needed for reading/writing a real rekordbox `master.db` (interop), which can be an optional feature.
- XML: `quick-xml` (streaming, handles UTF-16 via `encoding_rs`) for rekordbox XML, smart list XML, iTunes plist (or `plist` crate).
- Tags: `lofty` (pure Rust, MIT/Apache) for ID3v2/MP4/FLAC/Vorbis/AIFF/WAV read and write, artwork included. Replaces TagLib.
- Images: `image` + `fast_image_resize` for artwork variants; `jpeg-encoder` if needed.
- JSON: `serde`, `serde_json` (criteria, cues JSON, weights).
- Zip: `zip` crate (pure Rust with `deflate` via `miniz_oxide`/`flate2` rust backend).
- Paths/URIs: `url`, `percent-encoding` (match the XML Location reserved set).
- IDs/time: `uuid`, `rand`, `time` or `chrono` with UTC text format `YYYY-MM-DD hh:mm:ss.SSS +00:00`.
- File watching (optional, for relocation and sync): `notify`.
- Concurrency: `crossbeam-channel` or `tokio` for worker jobs; keep DB access on one writer thread with a pool of readers (WAL).
- Text collation for search/sort: `unicode-normalization`, `caseless`, plus our own natural-order compare.
- DeviceSQL `.edb` (v5 migration): no Rust prior art; implement later from Ghidra findings, behind a feature flag.

### 7.3 Parity test strategy

1. **Fixture libraries.** On a macOS test machine with rekordbox 7.2.19, build scripted fixture collections (a few dozen tracks covering every file type, tags, cues, hot cue banks, MyTags, smart playlists with every operator, histories, tag list, related-tracks sets). Export each as: copy of `master.db` (decrypted with our codec for diffing), rekordbox XML export, m3u8/txt exports, backup zip. Store only our own generated audio (sine sweeps etc.), never commercial audio. Keep fixtures outside the repo if they contain anything derived from AlphaTheta, otherwise under `tests/fixtures`.
2. **Read parity.** Open the fixture `master.db` with our crate and compare every row and column to a JSON dump made with pyrekordbox (independent implementation) and with `sqlcipher` CLI.
3. **Write parity.** Perform the same scripted operations in Cuepoint and in rekordbox (create playlist, reorder, delete synced/unsynced rows, add MyTag, edit smart list), then diff the resulting DBs table by table ignoring random IDs and timestamps but checking `Seq`/`TrackNo` renumbering, `rb_data_status`, `rb_local_usn`, soft-delete flags. Then reopen the Cuepoint-written DB in rekordbox and check it loads without "broken DB" handling.
4. **XML round trip.** Import rekordbox's XML into Cuepoint, export again, and compare canonicalised XML (attribute order and number formatting normalised). Also import Cuepoint XML into rekordbox and compare the resulting DB.
5. **Smart playlist evaluation.** For each fixture smart list, assert Cuepoint's matching track set equals rekordbox's (read from rekordbox's XML export of that playlist).
6. **Sort and search.** Capture rekordbox's sorted order for each column (via XML export of a playlist sorted then "save order", or screenshots via computer-use automation) and compare; same for keyword search hit sets.
7. **Tags.** Edit metadata in rekordbox with tag writing enabled and diff tag frames (with `lofty` and `mutagen`) against Cuepoint's writes.
8. **Backup/restore.** Restore a rekordbox backup zip in Cuepoint and vice versa; compare DB contents and relocated paths.
9. **Property tests.** `proptest` for renumbering, tree moves, soft/hard delete invariants, and XML Location URI encode/decode.

### 7.4 Interfaces to other subsystems

- Export (PDB/OneLibrary): reads tracks, playlists, cues, MyTags through `repo::` traits; writes `MasterDBID`/`MasterSongID` back for device matching.
- Analysis: owns ANLZ files; library stores `AnalysisDataPath`, `Analysed` bits, `*Updated` counters.
- Pro DJ Link DB server: implements `PSvDBMain`-style browse on top of `query::`.
- Cloud/sync: optional `agent` client, or Cuepoint's own sync; bookkeeping columns kept compatible either way.
- UI (egui): browser views bind to `query::` result sets with change notifications from a library event bus.
