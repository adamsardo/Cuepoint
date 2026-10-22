# 07 Online services, cloud, accounts and rekordboxAgent

Status: first pass from static evidence only (classes, demangled symbols, strings, bundled resources). No decompilation yet. Statements marked **(inferred)** are deductions from names and string context, not observed behaviour.

Evidence keys used below:

- `cls:X` a class (and its method list) in `analysis/classes.tsv`.
- `str:"..."` a string in `analysis/strings.txt` (secrets are never quoted; where one exists we say so).
- `agent:path` a file inside `rekordboxAgent.app/Contents/Resources/app.asar`.
- `bundle:path` a file in the unpacked `rekordbox.app/Contents`.

## 1. Purpose

This subsystem covers everything in rekordbox 7.2.19 that talks to a network service other than Pro DJ Link hardware:

1. Streaming catalogues playable on decks: Beatport (and legacy Beatsource), SoundCloud, TIDAL, Spotify, Apple Music, plus the Inflyte promo pool.
2. Cloud storage providers (Dropbox, Google Drive, a dormant OneDrive) used by Cloud Library Sync, Cloud Export and cloud-to-deck loading.
3. AlphaTheta's own cloud ("rekordbox Cloud", hosted under `cloud.kuvo.com`): account login (Gigya), subscription plans, activation and entitlements (Sentinel RMS / EMS), Cloud Library Sync metadata sync (AWS Lambda + S3 via the Electron agent), CloudDirectPlay for DJ players, collaborative ("shared") playlists, cloud analysis sharing, AI track recommendation, notifications, questionnaires and campaigns, product and owner registration.
4. Social upload of recordings (Mixcloud, YouTube).
5. Telemetry (in-house analytics, Bugsnag crash reporting, per-service play reporting).
6. The Electron helper `rekordboxAgent.app` and the updater `Upmgr rekordbox.app`.

For Cuepoint the goal is behavioural parity where it is legitimately achievable, an owned replacement where AlphaTheta's cloud is the only backend, and a clean drop where the feature exists only to enforce licences or collect telemetry. We do not circumvent DRM or licence checks, and we do not reuse any key, secret or client ID found in the binary.

## 2. Feature inventory

Checklist legend: `[ ]` to do. The trailing tag gives the category from section 5: **A** public/partner API we can apply for, **B** needs a commercial or DRM licence we cannot obtain or replicate, **C** AlphaTheta-proprietary cloud (own replacement), **D** drop.

### 2.1 Common streaming framework

- [ ] Central service registry routes a track URI or path to its service by scheme prefix (`soundcloud:tracks:`, `tidal:`, `spotify:`, `apple_music:`, `beatport`/`beatsource:tracks:`) and answers "is logged in" per service. **A** `cls:streaming::StreamingManager` (`getStreamingServiceFromPath isBeatPortProtocol isBeatSourceProtocol isSoundCloudProtocol isTidalProtocol isSpotifyProtocol isAppleMusicProtocol isStreamingLoggedIn`), `str:"soundcloud:tracks:"`, `str:"beatsource:tracks:"`, `str:"tidal:users:"`, `str:"spotify:users:"`, `str:"apple_music:users:"`
- [ ] Abstract service interface: search, categories, genres, curated playlists, charts, user library (playlists, liked tracks, artists, labels, albums), recent plays, related/similar tracks, playlist CRUD, preferred and highest audio quality, offline store, track extended info. **A** `cls:streaming::StreamingService`, `cls:streaming::StreamingAuth`
- [ ] Per-service capability flags that gate UI actions: create/edit/reorder/sort/pin playlists, like tracks, play on deck, track separation (stems) allowed, explicit content prohibited. **A** `cls:db::StreamingBridge` (`canCreatePlaylist canEditPlaylist canReorderPlaylist canSortPlaylist canPinPlaylist canLikedPlaylistTrack`), `cls:streaming::StreamingService` (`isAvailablePlayOnDeck isAvailableTrackSeparation isProhibitedExplicitTrack`)
- [ ] Library bridge per service maps streaming playlists and tracks into rekordbox browse rows (`RowDataTrack`, `ListDataPlaylist`) with stable local playlist IDs, async loaders, caches and invisible/unstreamable track handling. **A** `cls:db::StreamingBridge` (`getRbPlaylistIdFromStreamingPlaylistId hasUnstreamableTrackPlaylist findTrackFromUnstreamable getInvisiblePlaylistTracks`), `cls:StreamingHelper`, `cls:db::StreamingLoaderBase`, `cls:db::StreamingAsyncLoader`
- [ ] Async playlist operations with progress window: add/remove tracks, reorder, rename, delete, like/unlike, pin/unpin. **A** `cls:db::StreamingProgressWindowThread`, `cls:db::StreamingBridge` (`*Async`)
- [ ] Browse cache persisted to disk per service (charts, genres, curated, my collection, recommendations, playlists) for fast start and offline display. **A** `cls:streaming::StreamingService` (`saveCached* loadCached*`), `str:".soundcloud"`, `str:".tidal"`, `str:".spotify"`, `str:".applemusic"`
- [ ] Artwork URL cache and async image fetch. **A** `cls:StreamingHelper::ArtworkUrlCache`, `cls:db::StreamingBridge` (`addArtworkUrlCache`)
- [ ] Progressive download engine: per-deck download jobs, cancel per deck/URI, in-memory compressed buffer shared by reference count, playback can start before download completes, container parsers for FLAC, MP4 and Ogg. **A** `cls:streaming::DownloadManager`, `cls:streaming::DownloadThread` (`analyzeDownloadDataFLAC analyzeDownloadDataMP4 analyzeDownloadDataOgg`), `cls:streaming::CompressDataManager`, `cls:streaming::FragmentableData`, `str:"Range: bytes="`
- [ ] Download error taxonomy surfaced to UI: limit reached, storage error, licence error, stream error, rate limited with remaining seconds. **A** `cls:streaming::StreamingListener`, `str:"Connection: DOWNLOAD_ERROR_RATE_LIMITED: "`, `str:"DOWNLOAD_ERROR_TIMEOUT"`
- [ ] Audio quality selection per service and "quality when analysed" memory; warn if the deck plays a different quality than the analysis was made from (beat grid may differ). **A** `cls:db::StreamingBridge` (`saveAudioQualityWhenAnalyzed showAudioQualityDifferErrorIfNeed`), `str:"AudioQualityWhenAnalyzed"`, `str:"audio_quality_when_analyzed"`
- [ ] Offline mode: mark tracks for offline, offline track list, expiry, offline-only browse mode, online/offline switch, offline beacon queue sent on reconnect. **A/B per service** `cls:streaming::Offline`, `cls:db::StreamingBridge` (`m_OfflineTrackOnlyMode selectOfflineTracks updateOfflineTracks`), `cls:browse::StreamingOffLineButton`
- [ ] Auto-delete of streaming tracks from collection (housekeeping dialog). **A** `cls:StreamingAutoDeleteWindow`
- [ ] Deck event reporting hooks per service (load, play, pause, seek, speed change, scratch start/end, eject, loop, direction, edge reached, quit), used for royalty and play reporting. **A (contractual)** `cls:streaming::spotify::Service` (`send*Event`), `cls:streaming::apple_music::Service`, `cls:streaming::StreamingReporter`
- [ ] Streaming export restriction: streaming tracks cannot be exported to USB except in specific modes. **A** `cls:StreamingHelper` (`canUseStreamingExportMode canUseStreamingTypeExportMode`)
- [ ] Store button (buy track link) in browse. **A** `cls:browse::StreamingStoreButton`
- [ ] "Radar" localisation of service names per region. **A** `cls:db::StreamingRadarLocalizer`
- [ ] Track suggestion and related-track search across local and cloud catalogues. **A/C** `cls:db::TrackSuggestion` (`searchLocal searchCloud`), `cls:db::AsyncRelatedTrackSearch`
- [ ] Accept-Language header from UI language. **A** `cls:streaming::HttpAcceptLanguage`
- [ ] Network reachability watcher drives online/offline state. **A** `cls:common::NetworkWatcher`, `str:"http://connectivitycheck.gstatic.com/generate_..."`

### 2.2 Beatport and Beatsource

- [ ] Login via OAuth authorisation code in the system browser with a loopback redirect, also a trial login path, logout, "login required" recovery. **A** `cls:streaming::BeatportService`, `cls:db::BeatportBridge` (`login loginTrial logout`), `str:"{}/auth/o/authorize/?client_id={}&response_type=code&redirect_uri={}"`, `str:"/auth/o/token/"`, `str:"/auth/o/introspect/"`, `str:"http://localhost:55001/redirect.html"`
- [ ] Vendor SDK "libbeatport" v2.8.3 embedded (C++), with its own HTTP worker, sqlite store, logger and background heartbeat. **A** `cls:beatport::client`, `cls:beatport::BeatportClient`, `str:"User-Agent: libbeatport/v2.8.3"`, `str:"/my/link/heartbeat/"`, `str:"X-LINK-DEVICE-ID"`
- [ ] Licence object per user: start date, max decks, max quality, offline max tracks, full-track playback flag; licence is RSA-signature verified (public key embedded). **A** `str:"max_decks"`, `str:"max_quality"`, `str:"offline_max_tracks"`, `str:"full_track_playback"`, `cls:beatport::crypto::rsa` (`verify_signature`), `str:"/my/license/"`
- [ ] Catalogue browse: genres, top 100, curated playlists, charts, releases, artists, labels, artist/label top tracks, similar tracks, search (tracks, releases, artists, labels). **A** `cls:beatport::WebAPI`, `str:"/catalog/tracks/similar/"`, `str:"/top/100/"`, `str:"/curation/playlists/"`, `str:"<@base>/search/v1/{}"`
- [ ] My library: collection tracks/artists/labels, user playlists CRUD (create, rename, delete, add/remove/bulk, reorder by position). **A** `cls:beatport::WebAPI::Playlists`, `cls:beatport::WebAPI::PlaylistTracks`, `str:"/my/playlists/{}/tracks/bulk/"`
- [ ] Streaming quality per account (including lossless). **A** `str:"/my/streaming-quality/"`, `str:"lossless"`
- [ ] Track download: full track or two-minute preview; local cache with LRU by play count; offline "locker" with capacity limit; files stored encrypted (AES-CTR) per user secret. **A (follow vendor terms)** `cls:beatport::AssetManager`, `cls:beatport::crypto::aes::ctr256::Decrypter`, `str:"/catalog/tracks/{}/download/"`, `str:"2_min_preview"`, `str:"track_cache"`, `str:"track_locker"`
- [ ] Concurrent download limit, low speed timeout, CA bundle and logging configured via SDK config. **A** `str:"max_concurrent_downloads"`, `str:"http_low_speed_timeout"`
- [ ] ETag-cached paginated resources. **A** `cls:beatport::Etags`, `cls:beatport::ResourceCache`
- [ ] Play analytics events: session, play, add-to-playlist. **A (contractual)** `cls:beatport::PlaybackAnalytics`, `str:"/events/session/"`, `str:"/events/play/"`, `str:"/events/add-to-playlist/"`
- [ ] Staging and production environments for both brands. **A** `str:"https://api.beatport.com/v4"`, `str:"https://api.beatportstage.com/v4"`, `str:"https://api.beatsource.com/v4"`, `str:"https://api.beatsourcestage.com/v4"`
- [ ] Beatsource to Beatport migration: rewrites library track IDs via a migrator endpoint, deletes duplicates. **A** `cls:db::DatabaseMediator::BsToBpMigrationManager`, `str:"https://api.beatport.com/migrator/v1/track/"`, `cls:db::BeatportBridge` (`runBeatsourceToBeatportMigration`)
- [ ] Browse tree and login page UI for Beatport and Beatsource. **A** `cls:browse::BeatportBrowser`, `cls:browse::BeatportLoginPage`, `cls:browse::BeatsourceLoginPage`

### 2.3 SoundCloud

- [ ] OAuth 2 with PKCE (S256) through the browser and a local HTTP listener (two redirect ports), token refresh, account info and feature flags. **A** `cls:soundcloud::SoundCloudAuth` (`startHttpServer request_access_token request_refresh_token`), `str:"&code_challenge_method=S256"`, `str:"http://127.0.0.1:55000/redirect.html"`, `str:"http://127.0.0.1:55002/redirect.html"`, `str:"me/features"`, `str:"content_high_tier"`, `str:"dj_offline"`
- [ ] Two API hosts: public API and DJ partner API. **A** `str:"https://api.soundcloud.com"`, `str:"https://api-partners.soundcloud.com"`, `str:"https://secure.soundcloud.com"`
- [ ] Browse: charts/trending, genres, recommended, history (recently played), followings, likes (tracks and playlists), user playlists and albums, user uploads, related tracks, search. **A** `cls:soundcloud::SoundCloudBrowse`, `cls:db::SoundCloudBridge` (`getListTrack_Chart getListTrack_Following getListTrack_History getListTrack_Likes getListTrack_Recommend`)
- [ ] Playlist CRUD and like removal. **A** `cls:soundcloud::SoundCloudBrowse` (`createPlaylist addTrackToPlaylist removeTrackFromPlaylist updatePlaylistName deletePlaylists deleteLikedPlaylists`)
- [ ] Stream selection: HLS high/standard, progressive high quality, 128 kbps preview; MP3, MP4 and fragmented MP4 readers. **A** `str:"hls_hq"`, `str:"hls_sq"`, `str:"http_hq"`, `str:"preview_mp3_128_url"`, `cls:rbxfrm::SoundCloudMp3`, `cls:rbxfrm::SoundCloudMp4`, `cls:rbxfrm::SoundCloudFMP4`
- [ ] Anonymous playback for logged-out users and a limited "free playlist" with upsell; trial window; checkout link. **A** `cls:streaming::SoundCloudService` (`isAllowAnonymousPlayback requestTrial`), `cls:db::SoundCloudFreePlaylist`, `str:"https://checkout.soundcloud.com/dj"`
- [ ] Offline sync download (for DJ subscribers) with salted encryption at rest. **A/B (partner terms)** `cls:soundcloud::Offline` (`getSalt isEnableOfflineSyncDownload`)
- [ ] Play beacons (play, pause, checkpoint) and offline beacon catch-up. **A (contractual)** `cls:soundcloud::SoundCloudPlay`, `str:"beacons"`, `str:"checkpoint"`
- [ ] "API changed" dialog for breaking server changes. **A** `cls:db::SoundCloudBridge` (`showSoundCloudApiChangedDialog`)

### 2.4 TIDAL

- [ ] OAuth with PKCE via TIDAL login; the authorisation code is exchanged for tokens by AlphaTheta's cloud, not locally (server-held client secret, "SSO"). **A if own partner creds** `str:"https://login.tidal.com/authorize"`, `str:"https://auth.tidal.com/v1/oauth2/token"`, `str:"/client/v%d/ex_access_token/tidal/code2token"`, `cls:RekordboxCloud` (`postExtAccessTokenTIDALCode2Token setTidalSsoFunction`)
- [ ] Browse: featured (new, top, rising), genres, moods, curated playlists, My Mix and daily mix, favourites (tracks, albums, artists, playlists), artist albums and top tracks, album tracks, recommendations, search. **A** `cls:streaming::tidal::EditorialAPI`, `cls:streaming::tidal::MyMixAPI`, `cls:streaming::tidal::FavoritesAPI`, `str:"/v1/featured"`, `str:"/v1/moods"`, `str:"/v1/mixes/daily/track"`
- [ ] Playlist CRUD with ETag concurrency (If-None-Match), move items. **A** `cls:streaming::tidal::PlaylistsAPI` (`getPlaylistEtag moveItems`)
- [ ] Playback info after paywall: manifest types (TIDAL BTS JSON, HLS, DASH), qualities LOSSLESS and HI_RES, immersive formats (Dolby Atmos, Sony 360RA) excluded, licence security token. **A/B** `cls:streaming::tidal::TrackAPI` (`getPlaybackPostPaywall`), `str:"/playbackinfopostpaywall"`, `str:"application/vnd.tidal.bts"`, `str:"application/dash+xml"`, `str:"DOLBY_ATMOS"`, `str:"licenseSecurityToken"`
- [ ] Subscription check (Premium, Premium Plus, HiFi tiers), invalid plan error. **A** `str:"/subscription"`, `cls:db::TidalBridge` (`showInvalidPlanError`)
- [ ] Offline device authorisation with device limit, offline grace period, revalidation dates, offline play reports. **A/B** `cls:streaming::tidal::UserAPI` (`postOfflineDevice postReportOfflinePlays`), `str:"/clients/offline"`, `str:"offlineGracePeriod"`, `cls:db::TidalBridge` (`showOfflineDeviceLimitErrorWindow`)
- [ ] Image size catalogue for album/artist/genre/mood art. **A** `cls:streaming::tidal::ImageAPI`, `str:"https://resources.tidal.com/images/"`
- [ ] Beacon/event reporting. **A (contractual)** `cls:streaming::tidal::TidalSendBeaconJob`

### 2.5 Spotify

- [ ] OAuth via Spotify accounts with scopes for library read/modify, private and collaborative playlists, streaming and personalisation; code exchanged by AlphaTheta's cloud. **B** `str:"https://accounts.spotify.com/authorize"`, `str:"user-library-read"`, `str:"streaming"`, `str:"user-personalized"`, `str:"/client/v%d/ex_access_token/spotify/code2token"`
- [ ] Partner-only "DJ" API v2: me, capabilities (stem separation, offline mode), experiences (curated sections, lookup, search), collections (playlist and folder CRUD, reorder), library and pinned items, liked tracks. **B** `cls:streaming::spotify::DjAPI`, `cls:streaming::spotify::CollectionsAPI`, `cls:streaming::spotify::FunctionsAPI`, `str:"/v2/me/capabilities"`, `str:"/v2/experiences"`, `str:"/v2/collections"`, `str:"/v2/me/library/pinned"`, `str:"stem_separation"`
- [ ] Native partner download SDK ("DJDownload_*") with host callbacks for HTTPS, WebSocket, timers, clock, random and logging; client-token acquisition; storage resolve; formats Ogg Vorbis 96/160/320 and FLAC. **B** `cls:streaming::spotify::Client`, `cls:streaming::spotify::HttpsHalHelper`, `cls:streaming::spotify::WebSocketHalHelper`, `cls:ix::WebSocket`, `str:"DJDownload_init_connectivity"`, `str:"OGG_VORBIS_320"`, `str:"FLAC_FLAC"`
- [ ] Mandatory playback reporting (start, event, end, unexpected exit) persisted to a report file for retry. **B** `cls:streaming::spotify::PlaybackReportSaveFile`, `str:"/v1/playback/start"`, `str:"/v1/playback/end"`
- [ ] Explicit content filter honour; metadata refresh job; collection normalisation; sort preferences. **B** `str:"explicit_content"`, `cls:db::SpotifyMetadataRefreshAsyncLoader`, `cls:db::SpotifyCollectionNormalizeAsyncLoader`
- [ ] Database crypto for Spotify-derived fields at rest. **B** `cls:spotify_db_crypto`

### 2.6 Apple Music

- [ ] Login through Apple's web OAuth to obtain a Music User Token; developer token issued by AlphaTheta's cloud; storefront and explicit content policy. **B** `str:"https://activate.apple.com/oauth2"`, `str:"https://buy.itunes.apple.com/commerce/oauth2/token"`, `str:"/client/v%d/ex_access_token/apple/code2token"`, `str:"/client/v%d/ex_access_token/{service_type}/developer_token"`, `cls:streaming::apple_music::Auth` (`getMusicUserToken getStoreFront getExplicitContentPolicy`)
- [ ] Catalogue, library (playlists, albums, artists, songs, recently added), history, charts (global, city, top albums/playlists), Listen Now categories, DJ Mixes / "DJ Sounds Show" curator grouping, artist details, search. **B (API is public, playback is not)** `cls:streaming::apple_music::CatalogAPI`, `cls:streaming::apple_music::LibraryAPI`, `cls:streaming::apple_music::ListenNowAPI`, `str:"https://api.music.apple.com"`, `str:"/v1/me/library/playlists"`, `str:"cityCharts"`, `str:"DJ Mixes"`
- [ ] Playback: play assets with encrypted HLS, Widevine licence from Apple's web playback licence endpoint, decryption by castLabs CDM. **B** `cls:streaming::AppleMusicDrmLicenseRepository`, `str:"https://play.itunes.apple.com/WebObjects/MZPlay.woa/wa/acquireWebPlaybackLicense"`, `str:"/v1/play/assets"`, `str:"hlsEncryption"`
- [ ] Play activity reporting. **B** `str:"https://universal-activity-service.itunes.apple.com/play"`, `cls:streaming::apple_music::PlayActiveFeed`
- [ ] Explicit track discriminator for automix and browse. **B** `cls:db::AppleMusicExplicitDiscriminator`

### 2.7 HLS and DRM plumbing

- [ ] HLS master/media playlist parser: EXT-X-STREAM-INF, MEDIA, MAP, KEY, SESSION-KEY, SESSION-DATA, BYTERANGE; pick highest quality variant. **A for clear streams** `cls:streaming::hls::ManifestModel` (`GetHighestQualityMediaPlaylistUrl`), `cls:streaming::hls::PlaylistParser`, `str:"#EXT-X-SESSION-KEY"`
- [ ] Encryption methods recognised: AES-128 (clear key), SAMPLE-AES, SAMPLE-AES-CTR; key formats Widevine, FairPlay, PlayReady. **A only for AES-128 with a key the service legitimately gives us; B otherwise** `str:"AES-128"`, `str:"SAMPLE-AES-CTR"`, `str:"com.widevine.alpha"`, `str:"com.apple.fps"`, `str:"com.microsoft.playready"`
- [ ] Segment-pool downloader (header job, segment jobs, segment size). **A** `cls:streaming::DrmDecryptor` (`DownloadHeaderPoolJob DownloadSegmentPoolJob SetSegmentMb`)
- [ ] Widevine CDM from castLabs (bundled `clatc.framework`) initialised with a device keybox fetched from AlphaTheta's cloud. **B, never replicate** `cls:streaming::CastlabsDecryptorDrmCdmRepository`, `cls:db::RekordboxCloudKeyboxRepository`, `str:"/client/v%d/ex_access_token/castlabs/widevine_keybox"`, `str:"encrypted-keybox.bin"`, `bundle:Frameworks/clatc.framework`
- [ ] No-DRM path for clear content. **A** `cls:streaming::NoneDrmCdmRepository`

### 2.8 Inflyte (promo pool)

- [ ] Login to Inflyte, list promos and available tracks and formats, group by promo, download, remove from collection with confirmation. **A if partner** `cls:Inflyte`, `cls:InflyteAuth`, `cls:browse::InflyteBrowser`, `str:"https://inflyteapp.com"`
- [ ] "Inflyte Dropbox Sync": promo files arrive in a Dropbox namespace folder and are linked into the collection; JSON index of available tracks. **A** `cls:Inflyte` (`decideDropboxNamaspaceID dropboxFolder readAvailableTracks`), `cls:db::InflyteBridge`, `str:"Inflyte Dropbox Sync"`

### 2.9 Cloud storage providers

- [ ] Generic cloud storage service with OAuth sign-in, token refresh, account info, quota, file list, upload (simple, session, batch), download, move/copy, rename, delete, temp link, sharing info, revoke, desktop-app path detection. **A** `cls:ICloudService`, `cls:CloudService`, `cls:CloudOperation`, `cls:UploadOperation`, `cls:DownloadOperation`
- [ ] Dropbox API v2 including change long-polling, cursors, team/business root, desktop app detection. **A** `cls:DropBox`, `cls:DropboxAuth`, `str:"https://api.dropboxapi.com"`, `str:"https://content.dropboxapi.com/2/files/upload"`, `str:"files/list_folder/longpoll"`, `str:".dropbox/info.json"`
- [ ] Google Drive v3 with `drive.file` scope, About/quota, desktop mount detection, China endpoint. **A** `cls:GoogleDrive`, `cls:GoogleDriveAuth`, `str:"https://www.googleapis.com/drive/v3/files/"`, `str:"https://www.googleapis.com/auth/drive.file"`, `str:"com.google.drivefs"`, `str:"https://www.googleapis.cn"`
- [ ] OneDrive: auth endpoints and local folder detection only, no operations wired (dormant). **A (optional)** `cls:OneDrive`, `cls:OneDriveAuth`, `str:"https://graph.microsoft.com"`
- [ ] Cloud manager: active provider, encrypted token cache, files-on-cloud and downloaded indexes, to-do/doing queues, conflict callback, low space alerts, network change handling, OAuth redirect via custom URL scheme or loopback. **A** `cls:CloudManager`, `cls:CloudManager::Listener`, `str:"rekordboxdj://auth/redirect"`, `str:"http://127.0.0.1:55003/redirect.html"`
- [ ] Status bar upload/download progress. **A** `cls:common::statusbar::CloudUploadProgressBar`, `cls:common::statusbar::CloudProgressBar`

### 2.10 rekordbox Cloud account, plans and licensing

- [ ] Account login with email and password through Gigya, ID token exchanged for a rekordbox Cloud token; refresh token cached on disk; logout; forgot password and register links. **C** `cls:RekordboxCloudService` (`loginByIdToken postGigyaGetJWT postGigyaGetAccountInfo storeRefreshToken`), `cls:rb::middleware::webapi::gigya::GigyaUidRequest`, `str:"https://accounts.us1.gigya.com/accounts.login"`, `str:"api/v1/login_by_id_token"`, `str:"/client/v1/auth/token"`, Gigya API key present in strings (not recorded)
- [ ] Account button, avatar, account window. **C** `cls:AccountComponent`
- [ ] My Page window: tabs for account, plan overview (free plan, expiration, next billing), plan history, notifications with badge, caution states. **C** `cls:MyPageWindow`, `cls:PlanOverview`, `cls:PlanHistory`, `str:"/client/v2/licenses/mypage"`, `str:"/client/v2/licenses/subscriptions"`, `str:"/client/v2/licenses/subscription_histories"`
- [ ] Plans: Free, Core, Creative, Professional, Trial, plus the Cloud Library Sync option add-on. **D (unlock all)** `cls:RekordboxCloudService` (`namePlanFree namePlanCore namePlanCreative namePlanProfessional namePlanTrial nameOptionCLS`)
- [ ] Feature gate matrix answering "can use X" for every paid feature (DVS, video, lighting third-party DMX, MIDI learn, sampler sequence save, stretch, track separation, cloud export/sync/direct play, playlist sharing, SNS share, XML export, Groove Circuit, FX packs, phase, mix point link, etc.). **D (all on)**, but the list is the authoritative feature inventory for other subsystems. `cls:ActivationComponent` (`canUse*Function`)
- [ ] Local licence store via Sentinel RMS ("Kenri" = rights; "yuukouka" activate, "mukouka" invalidate, "koushin" renew, "koudoku" subscription) with lock codes, fingerprints and activation XML; cloud entitlements via Sentinel EMS proxied by AlphaTheta (bulk activate, permission ticket, revoke proof). **D** `cls:Kenri`, `cls:UnifiedApiRms`, `cls:RekordboxCloudService::EmsActivation`, `str:"https://pioneerdj.prod.sentinelcloud.com"`, `str:"/client/v1/ems/api/activations/bulkActivate"`, `str:"<sentinelQuery query=\"lockCodeInfo\" version=\"1.0\"/>"`
- [ ] Activate/deactivate this computer, async server licence validation, plan upgrade/downgrade listeners, expiry alerts, licence update progress. **D** `cls:ActivationComponent`, `cls:ActivationListener`, `cls:LicenseUpdateProgressWindow`
- [ ] Hardware unlock (dongle or connected DJ device grants features). **D** `cls:ActivationComponent` (`dongleDetected isDongleLicense canUseNonDongleDevices`)
- [ ] Newer v7 licence server: JWT access tokens, start/clear feature, plan fetch, MAC address filtering, device type and UDID. **D** `cls:rb::middleware::webapi::license::PlanRepository`, `cls:rb::middleware::webapi::license::JwtTokenDecoder`, `cls:jwt::verifier`, `str:"/api/client/v1/auth/token"`, `str:"/api/log/v1/filter_mac"`, `cls:rb::app::service::atc_feature::AtcFeatureComputerInfo`
- [ ] Trials: 30-day trial start, trial campaign configuration, trial dialogs, end-in-10-days notification, remaining days in status bar. **D** `cls:TrialCampaign`, `cls:TrialController`, `cls:TrialLicenseController`, `cls:TrialEndIn10DaysNotification`, `cls:StatusBarTrialRemainingDays`, `str:"/client/v1/campaign/configuration"`
- [ ] Upsell surfaces: plan introduction, paid plan recommendation, function lock notice, lock icons, advertisement window, plan lock/update cautions. **D** `cls:PlanIntroductionWindow`, `cls:PaidPlanRecomendationDialog`, `cls:FunctionLockNotification`, `cls:AdvertisementWindow`, `cls:PlanLockCaution`
- [ ] Product registration and owner registration of DJ hardware (VID/PID lookup in an encrypted product DB, serial, purchase date and channel), unlocks some v7 functions. **D** `cls:rb::app::service::product_registration::*`, `cls:rb::middleware::db::product::ProductDB`, `str:"/api/client/v1/product_registrations/registration"`, `str:"/api/client/v1/product_registrations/owner"`
- [ ] Cloud "registries" key/value store per account (stores song request list and tag list IDs among others). **C** `cls:RekordboxCloudService` (`getRegistries postRegistries postRegistriesClear`), `cls:rb::middleware::db::rekordbox::CloudAgentRegistry`, `str:"/client/v1/registries"`
- [ ] Remote config. **C** `str:"/client/v1/config"`, `cls:RekordboxCloudConfig`
- [ ] Notifications feed (in-app and via agent OS notifications). **D or C** `cls:NotificationComponent`, `str:"/client/v1/notifications"`
- [ ] Questionnaire dialog with privacy policy. **D** `cls:QuestionnaireDialogWindow`, `str:"/client/v1/questionnaires/update"`
- [ ] Gigya UID export/import to USB device or file (used by DJ players to identify the owner). **C (interop question)** `cls:GigyaUidManager` (`exportToDevice importFromDevice`)

### 2.11 Cloud Library Sync, Cloud Export, CloudDirectPlay

- [ ] Cloud Library Sync (CLS): library database rows synchronised between computers through the agent; audio files stored in the user's Dropbox or Google Drive; analysis, artwork and setting files stored in AlphaTheta's S3. **C** `cls:db::CloudAgentAPI`, `agent:jsc_arm64/controllers/sync_manager.jsc`, `cls:CloudLibrarySync`, `cls:CloudStorageService`, `str:"Cloud Library Sync"` (inferred split of file types from agent strings `contentFile`, `imageFile`, `settingFile`, `s3_read`)
- [ ] Sync modes: standalone, sync, trial sync, trial export sync; sync condition enabled/disabled; run/pause; "sync now". **C** `agent:jsc_arm64/config.json`
- [ ] Library change tracking columns on every synced table (`rb_local_usn`, `rb_data_status`, `rb_local_deleted`, `rb_local_synced`, `usn`) and a local update counter. **C (must preserve for interop)** `cls:rb::middleware::db::rekordbox::LibrarySync`, `str:"update agentRegistry set int_1 = %d where registry_id = 'localUpdateCount'"`
- [ ] Main app writes go through the agent's local API so they get a USN; long-poll for remote updates. **C** `cls:db::AppSyncDBController` (`LongPoll AppSyncDBJob`), `str:"http://127.0.0.1:30001/api/v1"`, `str:"/data/__TABLE_NAME__/%lu"`
- [ ] Auto-upload of the whole collection or of flagged playlists; default storage selection. **C** `cls:CloudStorageServiceAutoUploadComponent`, `cls:rb::app::service::cloud_library_sync::PlaylistAutoUploadFlag`, `cls:common::RbAppBackdoor` (`canCollectionAutoUpload`)
- [ ] Per-track cloud actions in browse: upload to cloud, download, delete local copy, delete from cloud, move from cloud ("Moved from Cloud" folder). **C** `cls:db::RowDataTrack` (`enableUploadToCloud enableCloudDownload enableLocalDelete enableCloudDelete enableMoveFromCloud`), `str:"PioneerDJ/Moved from Cloud"`
- [ ] Cloud to deck: loading a cloud-only track downloads it to a temp file and loads, then cleans up. **C** `cls:djplay::CloudToDeckManager`, `cls:djplay::CloudTrackDownloadThread`
- [ ] Cloud Export: separate playlist tree ("cloud export folder") for exporting to cloud, track upper limit per plan, duplicate name check, overwrite confirm, export result (success, fail, cancel, track limit). **C** `cls:rb::app::service::cloud_library_sync::*` (`CloudExportFolder CloudExportTrackUpperLimit PlaylistExporter PlaylistExportResult`), `cls:CloudExportDialog`
- [ ] CloudDirectPlay (CDP): DJ players log in to the user's cloud library; device authentication via an account key; the server prepares a player database; filter playlists chosen for CDP with a versioned filter; guide and settings pages. **C, likely not replaceable** `cls:CloudDJDeviceCloudDirectPlayComponent`, `cls:CloudDirectPlayFilterDialog`, `cls:LibrarySyncComponent::CloudDirectPlay`, `str:"/client/v1/player/auth/account_key"`, `str:"/client/v1/player/database/prepare"`, `str:"https://settings.rekordbox.com/clouddirectplay/disable-authenticationkey/"`
- [ ] Device library backup to cloud and restore. **C** `cls:CloudDeviceLibraryBackup`, `cls:ActivationComponent` (`canUseDeviceLibraryRestoreFunction`)
- [ ] Mobile library sync file settings (rekordbox for iOS/Android). **C** `cls:CloudMobileLibrarySyncFile`
- [ ] Cloud trial playlist with capacity limit and order correction. **D** `cls:CloudTrial`, `cls:CloudTrial::TrialPlaylist`
- [ ] Guided setup wizard for CLS and CDP including cloud login. **C** `cls:ClsCdpGuidanceWizardCloudLogin`, `cls:CloudLibrarySyncGuide`
- [ ] Shared DB merge when another computer changed the library. **C** `cls:db::DatabaseMediator::SharedDBMergeManager`

### 2.12 Collaborative (shared) playlists

- [ ] Share a playlist with up to N members by email invitation, join/dismiss, withdraw, remove/resolve users, edit history, members list, unshare (owner), notifications with join link. **C** `cls:PlaylistSharingService`, `cls:PlaylistSharingInviteComponent`, `cls:PlaylistSharingEditHistoryWindow`, `cls:PlaylistOwnerUnshareWindow`, `str:"/client/v1/shared_playlists/share"`, `str:"https://settings.rekordbox.com/sharedplaylist/invitations/"`
- [ ] Limits: playlists per account, members per playlist, tracks per playlist (defaults 100, 5, 1000 in agent config; server can override). **C** `agent:jsc_arm64/config.json`, `str:"max_member_count_per_playlist"`
- [ ] Agent-side sync of shared playlist contents and files with per-playlist data tokens. **C** `agent:jsc_arm64/controllers/playlist_sync_manager.jsc`

### 2.13 Cloud analysis and AI recommendation

- [ ] Cloud analysis share: search the cloud for a matching track's analysis and download it instead of analysing locally; upload own analysis. **C (drop or own service)** `cls:RekordboxCloud` (`analyzeTrackSearch analyzeTrackUpload analyzeConfiguration`), `cls:analyzer::AnalyzeJobCloudSearch`, `cls:analyzer::AnalyzeJobCloudDownload`, `str:"https://rb-share.kuvo.com/anlz/v1/track/search"`
- [ ] AI recommendation: register track embeddings, poll task status, query nearest neighbours with filters (genre, key, vocal), rate limits per day and minimum interval; results can include streaming providers' tracks. **C** `cls:AIRecommendCache`, `cls:AIRecommendFilterParams`, `str:"/client/v%d/recommendation/tracks/neighbors"`, `str:"max_call_per_day"`, `str:"min_call_interval_sec"`
- [ ] Older network recommend engine. **C** `cls:NetworkRecommend`, `cls:NetworkRecommendRestriction`

### 2.14 Social upload

- [ ] Upload selector for recorded mixes with per-service component, progress, picture, error formatting. **A** `cls:SnsUploadSelector`, `cls:SnsUploadSelector::SnsComponent`
- [ ] Mixcloud: OAuth, account info (pro/premium), upload audio with picture, tags, and timed tracklist sections (artist, song, start time). **A** `cls:MixCloud`, `cls:MixCloudAuth`, `str:"https://api.mixcloud.com"`, `str:"sections-%d-start_time"`, `str:"*.mp3;*.mp4;*.m4a;*.aac;*.ogg"`
- [ ] YouTube: Google OAuth with upload scope, channel lookup, upload settings window (title, description, privacy and similar). Audio is presumably combined with a still picture into a video **(inferred)**. **A** `cls:YouTube`, `cls:YouTubeAuth`, `cls:YouTubeUploadSettingWindow`, `str:"https://www.googleapis.com/auth/youtube.upload"`
- [ ] No SoundCloud upload path found in 7.2.19 (SoundCloud user uploads are only listed). `str:"/tracks/uploads"` used by `getPartnersUserUploadsTracks`

### 2.15 Legacy and dormant

- [ ] KUVO: account, DJ key, profile copy to devices, "make information public" live tracklists via CDJ/DJM HID map. Service believed discontinued (verify). **D** `cls:KuvoService`, `cls:KuvoAuth`, `cls:djplay::HidMapKUVO`, `str:"https://kuvo.com/kuvoapi/user/me/profile"`
- [ ] Amazon Music: translation units exist but contain no code. **D** `str:"__GLOBAL__sub_I_AmazonBridge.cpp"`
- [ ] OneDrive: see 2.9.

### 2.16 Telemetry

- [ ] In-house analytics: header (controller ID, licence, version, session, user), body of usage counters (service usage per streaming service, cloud sync type, export storage types, deck loads, layout, language, OS, feature usage such as smart cue, track separation, vocal detect), monthly report, initial send at startup, background send thread. **D** `cls:Analytics::Manager`, `cls:Analytics::Helper`, `cls:Analytics::HeaderInfo`, `cls:Analytics::MonthlyReportManager`, `cls:rb::middleware::db::rekordbox::MonthlyReportService`, `str:"https://cloud.kuvo.com/api/log/v1/log"` (endpoint association inferred)
- [ ] Streaming music usage sender (service and DRM type per action). **D** `cls:db::StreamingMusicUsageSender`
- [ ] Bugsnag crash reporting with device, audio and OpenGL metadata and breadcrumbs. **D** `cls:BugsnagWrapper`, `cls:RekordboxBugsnagListener`, `bundle:Frameworks/Bugsnag.framework`
- [ ] libbeatport route analytics. **D (unless contractually required)** `cls:analytics::route_analytics`

### 2.17 Updater and downloadable content

- [ ] Update manager app: checks version, lists update packages, downloads, requires administrator rights, quits and relaunches rekordbox; launched and terminated by the main app. **C (own updater)** `bundle:MacOS/Upmgr rekordbox.app` (bundle ID `com.pioneer.Upmgr_rekordbox`, version 3.0.1.2), `str:"Upmgr error in NSTask! with error :%@ "`, `str:"__GLOBAL__sub_I_LaunchUpmgr.mm"`
- [ ] Content downloads from `dl.rekordbox.com` (stem models, visualiser packs, lyric font, sample packs). Cross-reference other subsystems; we never fetch AlphaTheta assets. **D** `str:"https://dl.rekordbox.com/application/models/stems/"`

## 3. Endpoints and protocols

No keys, client IDs, client secrets or passwords are recorded here. Where they exist in the binary this is noted.

### 3.1 AlphaTheta cloud ("Cloud API")

Base host `https://cloud.kuvo.com`, paths under `/api` (`str:"https://cloud.kuvo.com/api"`). Paths are built as `/client/v%d/...`; observed versions v1 and v2.

| Area | Paths (method inferred from class names) | Notes |
|---|---|---|
| Identity | Gigya `accounts.login` on `accounts.us1.gigya.com` (apiKey, loginID, password) then `api/v1/login_by_id_token` | Gigya API key embedded in strings |
| Tokens | `/client/v1/auth/token` (grant types for password, refresh), `/client/v1/gigya/accounts_getJWT`, `/client/v1/gigya/accounts_getAccountInfo` | Basic auth with an embedded client credential plus Bearer for user calls; refresh token cached to a file (`refreshTokenCacheFileName`) |
| Profile | `/client/v%d/profile`, `/client/v%d/dropbox/profile` | |
| Licences | `/client/v2/licenses/mypage`, `/subscriptions`, `/subscription_histories`, `/client/v1/licenses/activations` | JSON fields `your_product_id`, `subscription_status`, `next_billing_datetime`, `license_key`, `options` |
| EMS proxy | `/client/v1/ems/api/entitlements/eid=`, `/activations/bulkActivate`, `/activations/aId=.../generatePermissionTicket`, `/submitRevokeProof` | Sentinel EMS JSON bodies with device name and type |
| External tokens | `/client/v%d/ex_access_token/{service_type}`, `/update`, `/delete`, `/developer_token`, `/configuration`, `/google/code2token`, `/tidal/code2token`, `/spotify/code2token`, `/apple/code2token`, `/castlabs/widevine_keybox` | Server holds third-party client secrets and stores per-user service tokens ("SSO" across computers) |
| Cloud status | `/client/v%d/cloud/status`, `/cloud/trial/start`, `/cloud/trial/status` | |
| Players (CDP) | `/client/v1/player/auth/account_key` (+ `/all`, `/validate`), `/client/v1/player/database/prepare` (+ `/status`, `/cancel`) | |
| Sharing | `/client/v1/shared_playlists/` + `share`, `dismiss`, `withdraw`, `{uuid}/users/remove`, `{uuid}/users/resolve`, `{uuid}/histories` | |
| Misc | `/client/v1/registries` (+ `/clear`), `/client/v1/config`, `/client/v1/notifications`, `/client/v1/questionnaires` (+ `/update`), `/client/v1/campaign/configuration`, `/api/client/v1/server_time` | |
| Recommendation | `/client/v%d/recommendation/embedding/register`, `/embedding/status`, `/tracks/neighbors`, `/tasks/{id}` | async task model: `task_id`, `finished`, `execution_error` |
| Agent auth | `/api/client/v1/auth/validate`, `/api/client/v1/aws/id_token`, `/api/client/v1/aws/anonymous_id_token` | used by the agent (`agent:.../auth_manager.jsc`) |
| Logging | `/api/log/v1/log?platform=rekordbox7&client=rekordbox&version=...`, `/api/log/v1/filter_mac` | basic-auth "log endpoint" credential embedded |
| Licence v7 | `/api/client/v1/auth/token`, product registration paths under `/api/client/v1/product_registrations/` (`registration`, `registration/status`, `owner`, `dialog_skip`) | host for `LicenseServer` not confirmed (open question) |

Other AlphaTheta hosts: `rb-share.kuvo.com/anlz/v1/{configuration,track/search,track/upload}` (cloud analysis, multipart upload with BPM, key, gain, peak, phrase, analysis hash), `settings.rekordbox.com` (web settings pages), `rekordbox.com` (account, plan, FAQ, manual links), `kuvo.com` (legacy), `pioneerdj.prod.sentinelcloud.com` (Sentinel), `dl.rekordbox.com` (content).

### 3.2 Cloud Library Sync data plane (agent)

- The agent obtains AWS web-identity credentials (Cognito, region eu-west-1) from the Cloud API using the user's access token, or anonymous credentials when logged out (`agent:auth_manager.jsc` strings `WebIdentityCredentials`, `aws/id_token`, `aws/anonymous_id_token`).
- Metadata sync calls AWS Lambda functions named `ContentStore_getSyncState`, `ContentStore_getSyncChunk`, `ContentStore_mutateItems`, `ContentStore_deleteItems`, `ContentStore_notifyUpdate`, passing a client context (`agent:sync_manager.jsc`). The backing store is presumably DynamoDB (`dynamo_rw`) **(inferred)**.
- Files (analysis, artwork, settings) go to an S3 bucket in eu-west-1 named in the agent config, with object tagging and a `public-read` ACL string present (`s3_read`, `s3_write`, `putObject`, `getObjectTagging`).
- Sync algorithm: full sync and incremental sync in chunks keyed by server USN; local changes tracked by local USN; send-changes in chunks; states `IN_SYNC`, `SYNC_MODIFIED`, `SYNC_DELETED`, `IN_SYNC_DELETED`, `LOCAL_ONLY`, `LOCAL_DELETED`; conflict cases "rebirth", soft delete, hard delete; post-sync repairs (album de-duplication, My Tag merge by name and cap, tree validation, renumbering, orphan cleanup of cues, active censors and hot cue bank lists). Clock skew is corrected against `server_time`.
- Shared playlists use a per-playlist "data token" and endpoints `/data/token`, `/data/contents/check_existence`, `/data/files`, `/data/files/tags` (host is the Cloud API, exact base not visible) (`agent:playlist_sync_manager.jsc`).

### 3.3 Third-party services

| Service | Hosts | Auth | Data |
|---|---|---|---|
| Beatport / Beatsource | `api.beatport.com/v4`, `api.beatsource.com/v4` (+ stage hosts), `stream.beatport.com`, `geo-media.beatport.com` | OAuth code flow, loopback `localhost:55001`, introspect, refresh; RSA-signed licence; app identifier, key pair and redirect in `cls:BeatportConfig` | catalogue, `/my/*` library, downloads, events, link heartbeat |
| SoundCloud | `api.soundcloud.com`, `api-partners.soundcloud.com`, `secure.soundcloud.com`, `*.sndcdn.com` | OAuth + PKCE S256, loopback `127.0.0.1:55000` and `:55002`, header `Authorization: OAuth <token>`; client ID and secret in `cls:SoundCloudConfig` | me, features, tracks, playlists, likes, followings, trending, genres, streams, beacons |
| TIDAL | `login.tidal.com`, `auth.tidal.com/v1/oauth2`, `api.tidal.com/v1`, `listen.tidal.com`, `resources.tidal.com` | OAuth + PKCE, code exchanged via Cloud API; `x-tidal-token` client token header; client ID in `cls:streaming::TidalConfig` | users, favourites, playlists (ETag), featured, genres, moods, mixes, playbackinfopostpaywall, offline clients, offline play reports, sessions |
| Spotify | `accounts.spotify.com`, `api.spotify.com`, `partner.spotify.dev`, `gullwing.spotify.com`, `clienttoken.spotify.com`, `spclient.wg.spotify.com` | OAuth (scopes listed in 2.5), code exchanged via Cloud API, client token; client ID in `cls:streaming::SpotifyConfig` | v2 partner API, storage-resolve, connect-state, playback reporting |
| Apple Music | `activate.apple.com/oauth2`, `buy.itunes.apple.com/commerce/oauth2`, `api.music.apple.com`, `play.itunes.apple.com`, `universal-activity-service.itunes.apple.com` | developer token from Cloud API, `Media-User-Token`/`Music-User-Token` headers; client credentials in `cls:streaming::AppleMusicConfig` | catalogue, library, charts, play assets, Widevine licence, play activity |
| Dropbox | `api.dropboxapi.com`, `content.dropboxapi.com`, `notify.dropboxapi.com`, `www.dropbox.com/oauth2` | OAuth with offline refresh token, revoke | files, users, longpoll |
| Google | `accounts.google.com/o/oauth2`, `oauth2.googleapis.com`, `www.googleapis.com` (Drive v3, YouTube) | OAuth, `access_type=offline`, `prompt=consent`, API key present | Drive files, About; YouTube channel and upload |
| Mixcloud | `www.mixcloud.com/oauth`, `api.mixcloud.com` | OAuth, `access_token` query parameter; client credentials embedded | `me/`, `upload/` multipart |
| Inflyte | `inflyteapp.com` | own login window | promos, tracks, Dropbox namespace |
| Bugsnag | SDK defaults | API key presumably in app config (not inspected) | crash reports |

Local redirect HTML pages titled "rekordbox http server" auto-close after three seconds and bounce to the service's website (`str:"<title>rekordbox http server</title>"`).

### 3.4 Local IPC

- Main app to agent: HTTP on `127.0.0.1:30001`, path prefix `/api/v1`, `Authorization: Bearer <token>` (see section 4).
- OAuth redirects: loopback HTTP listeners (55000 to 55003) and the `rekordboxdj://` URL scheme (`bundle:Info.plist` CFBundleURLSchemes, `cls:CloudManager` `customUriSchemeHandler`).
- Agent notification deep links: `rekordboxagent://notification/<id>/`.

## 4. rekordboxAgent (Electron)

### 4.1 Packaging

- `rekordboxAgent.app`, bundle ID `com.electron.rekordboxagent`, version 3.3.0, `LSUIElement` and `LSBackgroundOnly` (menu bar only), Electron 19 (dev dependency), Squirrel/Mantle/ReactiveObjC frameworks bundled but no auto-update use observed. `agent:package.json` (name `rb-cloud-agent`).
- Source is shipped as V8 bytecode compiled with `bytenode` in two sets (`jsc_arm64/`, `jsc_x64/`); `main.js` only selects the set and loads `main_core.jsc`. Plain-text resources: `config.json`, `statuses.json`, a React notification window bundle, `preload.js`, a mock DB `data/db_mock.db`.
- Dependencies of note: `express` 4 with `express-bearer-token`, `sequelize` 4 over `@journeyapps/sqlcipher` (opens the encrypted master library directly), `aws-sdk` v2, `request-promise-native`, `md5-file`, `jimp-compact`, `egoroof-blowfish`, `electron-json-storage`, `electron-log`, `ps-list`, `node-powershell` and `winreg` (Windows), `xml-js`.

### 4.2 Lifecycle and IPC with rekordbox

- rekordbox writes a LaunchAgent plist (`Label` = `com.pioneerdj.rekordboxdj.agent`, `RunAtLoad`, runs `open <agent app>`) into `~/Library/LaunchAgents` and calls `launchctl` (`str:"Library/LaunchAgents"`, `str:"/bin/launchctl"`). It launches the agent with `--db-path`, `--port`, `--analysis-data-root-path`, `--settings-root-path`, `--lang-path`, `--lang`, `--app_ver`. Options persist in `rekordboxAgent/storage/options.json`. `cls:db::CloudAgentAPI::Agent` (`start quit registStartup portNum token url`).
- The agent enforces a single instance, checks that a rekordbox process exists, records window geometry, and keeps running after rekordbox quits so sync can finish (tray "Quit rekordbox" item) **(inferred from tray strings)**.
- Main to agent handshake: `POST /agent/login` with the cloud access token and a secret key; `/agent/token` to generate or revoke the local API bearer token. rekordbox also inspects `netstat -an` for LISTEN/ESTABLISHED on the port (`str:"/usr/sbin/netstat -an"`).
- Renderer IPC (notification window, via `contextBridge` `electronAPI`): `renderer-ready`, `client-ready`, `client-init`, `data-updated`, `content-size`, `show-always-notification-window`, `item-click` (`agent:public/shared/preload.js`).

### 4.3 Local HTTP API (Express, port 30001)

Derived from route file names (`agent:jsc_arm64/routes/api/**`). Responses use `statuses.json`: success `{result: true, code: 0}`, errors 400 bad params, 401 unauthorised, 404 endpoint or item not found, 500 server.

- `agent/`: `login`, `request_quit`, `env` (get/set: DB path, analysis root, settings root, language path), `mode` (get/set), `syncCondition` (get/set), `sharedPlaylists/mode`, `sharedPlaylists/syncCondition`, `showNotification`, `messages` (list), `token` (generate/revoke).
- `data/`: per-table CRUD with sync bookkeeping for `djmdContents` (get, list, create, update, delete, file request/touch/delete), `djmdCues` (list, list by kind, CRUD), `djmdActiveCensors`, `djmdPlaylists` (create, move, file ops), `djmdCloudExportPlaylists`, `djmdHotCueBanklists` (file ops), `djmdProperties`, `djmdRecommendLikes` (bulk delete), `djmdSongHotCueBanklists`, `djmdSongRequestLists`, `djmdSongTagLists` (create, update, delete-and-renumber, renumber), and generic `simpleItems`, `hierarchyItems` (move, renumber), `songRelationItems`, `settingFiles` (touch, delete).
- `localSync/`: `prepare`, `close`, `state`, `chunk meta`, `chunk`, `chunk for table`, `mutate items`, `get file`, `put file`, `get setting file`. Purpose: local peer sync, plausibly for rekordbox mobile on the same network **(inferred)**.
- `sharedPlaylists/`: `request_fetch`, `request_upload_data`.

### 4.4 Controllers (background workers)

- `auth_manager`: cloud token validation, AWS credential refresh, unrecoverable-auth state, UID change detection.
- `sync_manager`: Cloud Library Sync engine described in 3.2.
- `playlist_sync_manager`: shared playlists sync.
- `file_copy_worker`, `file_download_worker`, `image_file_*_worker`: prioritised file transfer queues (`rb_priority`, `rb_in_progress`, `rb_local_file_status`), hashing and size checks.
- `notification_manager` and `notification_log_worker`: periodic fetch of `/api/client/v1/notifications` and agent config, OS notifications (macOS and Windows toast), read state, image cache.
- `tray_manager`: tray states Online/Offline x Idle/Running/Paused/Standalone/Disabled with unread variants; menu: sync now, pause/resume, shared playlist sync, notification and sound toggles, quit rekordbox; staging label.
- `strings_manager`, `log_worker`; libs for connectivity probe, clock server, semaphores, key/value store.

### 4.5 What it serves

Cloud Library Sync, collaborative playlists, Cloud Export trial modes, cloud notifications, and a local data API that serialises library writes from rekordbox. Without an AlphaTheta account the agent stays in `standalone` mode.

## 5. Classification

| Service / feature | Cat. | Plan for Cuepoint |
|---|---|---|
| Beatport streaming (incl. offline locker) | A | Apply for Beatport partner API access. Implement against their documented v4 API with our own client credentials, honour licence limits (decks, quality, offline cap) and mandatory event reporting. Never reuse the embedded SDK keys. Feature-gated until we have credentials. |
| Beatsource | A / D | Beatsource streaming has been folded into Beatport per the migration code; implement only the ID migration reader if users import rekordbox libraries with Beatsource tracks. |
| SoundCloud (public API) | A | Use the public API (OAuth 2.1 + PKCE) for browse, likes, playlists, search. |
| SoundCloud DJ partner API, high quality and offline | A (partner) | Apply for SoundCloud DJ partner access; until granted, browse only and play whatever the public stream endpoints permit. |
| TIDAL | A (partner) | TIDAL's public developer platform covers catalogue; full-track DJ streaming and offline need a partner agreement. Do the code exchange ourselves with our own credentials (no AlphaTheta proxy). Clear or partner-provided streams only. |
| Spotify | B | DJ playback runs through a closed partner SDK and partner-only v2 endpoints. Cannot replicate. Offer at most library import of playlist metadata via the public Web API for track matching, no playback. |
| Apple Music | B | Catalogue and library browsing is possible with our own MusicKit developer token, but deck playback requires Widevine/FairPlay licence decryption. Do not implement playback. Optional: metadata import only. |
| castLabs Widevine CDM, keybox, any SAMPLE-AES / CENC decryption | B | Never implement. Detect encrypted variants and refuse with a clear message. |
| HLS (clear AES-128 or unencrypted) | A | Implement for services that legitimately serve clear HLS (for example SoundCloud). |
| Inflyte | A (partner) | Contact Inflyte; otherwise support "watch a folder" so Inflyte's own Dropbox delivery still lands in the library. |
| Dropbox, Google Drive, OneDrive | A | Implement via official APIs with our own app registrations, scoped to an app folder. Used as storage for our own sync. |
| Mixcloud upload | A | Implement with our own Mixcloud app: OAuth, multipart upload, tracklist sections from history. |
| YouTube upload | A | Implement with Google OAuth (`youtube.upload`); render a still-image video locally (ffmpeg-free path via our video subsystem or skip). Requires Google app verification. |
| rekordbox Cloud account (Gigya, Cloud API tokens) | C | No account system. Local profile only. Optional future self-hosted account for our own sync server. |
| Cloud Library Sync | C | Own replacement: library change journal synced through the user's own storage (Dropbox, Google Drive, OneDrive, S3, WebDAV or a plain folder). Audio and analysis files mirrored alongside. See section 7. |
| Cloud Export, cloud-to-deck, auto-upload, move from cloud | C | Rebuild on top of our storage backends (no plan limits). |
| CloudDirectPlay, player account keys, player database prepare, device library backup | C (no viable replacement) | DJ player firmware talks to AlphaTheta's servers, so we cannot substitute. Drop. Keep USB/Pro DJ Link export as the path to players. |
| Collaborative shared playlists | C | Own replacement: share a playlist file through a shared folder in the user's storage (read-only first, then merge with per-member change logs). |
| Cloud analysis share (rb-share) | C | Drop for v1; possibly an opt-in community analysis cache later. Never upload user data by default. |
| AI recommendation (embeddings, neighbours) | C | Replace with local similarity search over our own audio features (analysis subsystem); no cloud. |
| Registries, remote config, campaigns, questionnaires, notifications, ads, upsell | D | Drop. Release notes shown locally from the bundled changelog. |
| Plans, activation, Kenri/Sentinel RMS, EMS, licence v7 JWT, trials, hardware unlock, product/owner registration | D | Drop entirely; all features available. Use the `canUse*Function` list only as a feature inventory. |
| Analytics, monthly report, streaming usage sender, Bugsnag, libbeatport route analytics | D | Drop. Optional local crash log the user can attach to an issue. |
| Per-service play reporting (Beatport events, SoundCloud and TIDAL beacons) | A (contractual) | Implement only when we hold that service's partner agreement and only as the agreement requires. Not general telemetry. |
| KUVO | D | Drop (legacy). |
| Amazon Music | D | Not present in practice. |
| Upmgr updater | C | Own optional update check (signed release manifest), no admin installer. |
| dl.rekordbox.com content | D | Never fetch AlphaTheta assets; other subsystems supply their own models and packs. |
| rekordboxAgent | C | Not reproduced as a separate Electron app. Equivalent logic becomes an in-process sync worker (optionally a small background daemon), see section 7. |

## 6. Open questions for Ghidra

Do not decompile anything in category B or D beyond what is needed to describe user-visible behaviour. Concrete targets:

1. `RekordboxCloudService::getCloudApiURL` and `RekordboxCloudService::getConfig`: production vs staging selection and which path versions are live.
2. `rb::middleware::webapi::license::LicenseServer::Get` / `::Post`: base host of the v7 licence and product registration API (only to document, not to call).
3. `db::CloudAgentAPI::Agent::start`, `::login`, `::token`, `::portNum`: exact handshake order, how the secret key is derived, port fallback if 30001 is taken.
4. `db::AppSyncDBController::LongPoll::run`: which agent endpoint is long-polled and the change notification payload.
5. `rb::middleware::db::rekordbox::LibrarySync` constants and `LibrarySyncSql`: full meaning of `rb_data_status` values (257 observed) and the `rb_local_*` columns, so Cuepoint writes do not corrupt a library still used with CLS.
6. `streaming::StreamingManager::getStreamingServiceFromPath` and `::getUrn`: complete URI grammar for streaming tracks stored in `djmdContent`.
7. `streaming::DownloadThread::analyzeDownloadDataFLAC`, `::analyzeDownloadDataMP4`, `::analyzeDownloadDataOgg`, `streaming::DownloadManager::getDownloadBufSize`: how much data must be buffered before a deck may play, and seek behaviour before download completes.
8. `db::StreamingBridge::showAudioQualityDifferErrorIfNeed`: rule for warning about analysis at a different quality.
9. `ActivationComponent::canUseFunction` and each `canUse*Function`: the feature list and default for Free plan, to confirm the inventory used by other subsystems.
10. `rb::app::service::cloud_library_sync::CloudExportTrackUpperLimit::GetMaxNumberOfTracks`, `CloudTrial::getTrialListCapacity`: limits (to document; we will not enforce).
11. `PlaylistSharingService::syncSharingPlaylist`, `::postShare`: invitation payload and how member edits are merged.
12. `RekordboxCloud::analyzeTrackSearch`, `::fillAnalysisData`: what identifies a track (hash, duration, tags) for cloud analysis lookup.
13. `AIRecommendCache::requestUpdateEmbeddingStatus`, `RekordboxCloud::postRecommendationEmbeddingRegister`: whether embeddings are computed locally (possibly via `nc::` NumCpp) or server side.
14. `GigyaUidManager::exportToDevice`, `browse::CloudDirectPlayButtonItemComponent::loadAccountKey`: what is written to USB media for players, so our USB export can avoid clobbering it.
15. `SnsUploadSelector::SnsComponent::createPicture`, `YouTube::upload`: whether YouTube gets a rendered video or audio plus image, and accepted formats.
16. `MixCloud::Upload::createInputStream`: how the tracklist sections are built from history.
17. `Inflyte::readAvailableTracks`, `Inflyte::decideDropboxNamaspaceID`: JSON schema and folder layout of Inflyte's Dropbox delivery.
18. `CloudManager::customUriSchemeHandler`: `rekordboxdj://` routes besides `auth/redirect`.
19. `LaunchUpmgr` (Objective-C++ unit `LaunchUpmgr.mm`) and the `Upmgr rekordbox` binary: update feed URL and package manifest format.
20. `rb_http_server` (Go binary in `MacOS/`): its role. Candidates are OAuth redirect hosting or a local device/mobile endpoint.
21. `Analytics::Manager::Send` and `db::StreamingMusicUsageSender::Action`: confirm endpoint and payload so the drop list is complete.
22. `common::RbAppBackdoor`: debug switches that alter cloud behaviour (trial flag, auto-upload, move on cloud share).

## 7. Rust plan

### 7.1 Crate layout

All of this lives in `crates/cuepoint-services`, split by feature flags so that builds without partner credentials compile and run.

- `http`: one shared `reqwest` client with `rustls` (webpki roots plus platform verifier), HTTP/2, gzip, retry with jittered backoff honouring `Retry-After`, per-host rate limiter (`governor`), request logging with secrets redacted.
- `auth`: `oauth2` crate (authorisation code + PKCE S256, refresh), loopback redirect listener on `127.0.0.1` with an ephemeral port (`tokio` + `hyper` minimal handler), optional `cuepoint://` scheme handler per OS, tokens stored in the OS keychain via `keyring`, never in the library DB. Client IDs come from a user-editable config or build-time env vars, never committed.
- `streaming`: `trait StreamingProvider` with `capabilities()`, `search`, `browse(section)`, `playlists` CRUD, `liked`, `resolve_stream(track, quality) -> StreamSource`, `report(event)`; `StreamSource` is one of progressive HTTP, clear HLS, or `Refused(reason)` for DRM. Providers: `beatport` (feature `beatport`), `soundcloud`, `tidal` (feature `tidal`). `spotify` and `apple_music` limited to optional metadata import (`feature = "import-only"`).
- `hls`: `m3u8-rs` parsing, variant selection, byte-range and segment fetch, AES-128 CBC via `aes` + `cbc` for clear-key streams only. Any `SAMPLE-AES*`, `KEYFORMAT` Widevine/FairPlay/PlayReady, or CENC result is refused.
- `download`: progressive buffer shared with the engine (`bytes::Bytes` ring plus a `watch` channel for "readable up to N"), container probing via `symphonia` (FLAC, MP4/AAC, Ogg), cache directory with LRU eviction, optional encrypted offline store only if a partner contract demands it (key from the keychain).
- `storage`: `opendal` services for Dropbox, Google Drive, OneDrive, S3, WebDAV and local filesystem behind one `trait Storage`, plus change watching (Dropbox longpoll, Drive changes, polling fallback).
- `sync`: Cuepoint Library Sync. Each device appends signed, ordered operation logs (`ops/<device-id>/<seq>.cbor`, `serde` + `ciborium`) to the chosen storage; a hybrid logical clock orders operations; deterministic merge rules mirror the repairs rekordbox performs (dedupe albums, merge tags by name, renumber sequences, repair orphan cues). Audio, analysis and artwork are content-addressed (`blake3`) under `blobs/`. Snapshots compact old logs. No server required; a self-hosted relay can come later.
- `sharing`: shared playlist bundles in a shared storage folder (read-only first).
- `social`: Mixcloud and YouTube uploaders (multipart, resumable for YouTube).
- `rekordbox_compat`: read-only understanding of `rb_local_usn`, `rb_data_status` and related columns so importing a CLS-enabled rekordbox library preserves them untouched (shared with the library subsystem).
- `update`: optional release check against a signed manifest (`minisign-verify`), user-initiated install only.

Explicitly not built: accounts, plan or licence logic, telemetry, Bugsnag, Sentinel, Gigya, CloudDirectPlay, AlphaTheta cloud clients, DRM.

### 7.2 Parity test strategy

- Contract tests per provider with recorded fixtures (`wiremock`) built from each service's public documentation, not from rekordbox traffic; live tests behind an env flag against partner sandboxes once we hold credentials (Beatport publishes stage hosts).
- HLS: unit tests over synthetic master/media playlists covering every tag rekordbox recognises; assert that encrypted variants yield `Refused`.
- Progressive playback: property tests that the engine never reads past the readable watermark; seek-before-complete tests using throttled local HTTP servers.
- Sync: `proptest` model tests that N simulated devices applying random edits, offline periods and reorderings converge to the same library; regression fixtures for each repair rule listed in 3.2.
- Storage backends: `opendal` memory and local backends in CI; Dropbox and Drive behind credentials in a nightly job.
- rekordbox interop: import a rekordbox 7.2.19 `master.db` with CLS columns, export again, and diff that sync columns and unrelated rows are byte-identical.
- UI parity: a checklist walk-through comparing the browse tree sections, context menu actions and dialogs per service against section 2, ticking only items in categories A and C.
