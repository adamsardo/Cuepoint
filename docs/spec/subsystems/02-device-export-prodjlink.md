# 02. Device export, CDJ/XDJ interop and Pro DJ Link

Status: first draft from static evidence (symbols, class list, strings, helper binaries) of rekordbox 7.2.19 for macOS. No decompilation has been done yet. Statements marked **(inferred)** are deductions from names or context, not confirmed behaviour. Statements marked **(prior art)** come from public reverse-engineering projects, not from the rekordbox binary.

Evidence notation: backticked names are C++ classes or methods (`Class::method`), string literals as they appear in the binary (`"..."`), or helper executables. Raw evidence lives in `/opt/rekordbox/7.2.19/analysis` and is not part of this repo.

## 1. Purpose

This subsystem gets a rekordbox library onto AlphaTheta/Pioneer DJ hardware and keeps rekordbox and that hardware talking to each other. It covers four jobs:

1. **USB/SD export ("USB EXPORT").** Writing a self-contained library to removable media: music files, the legacy DeviceSQL database (`export.pdb`, `exportExt.pdb`), the newer SQLCipher database (`exportLibrary.db`, marketed as Device Library Plus and since late 2025 as OneLibrary), per-track ANLZ analysis files, artwork, and player/mixer settings files. Also reading devices back (import playlists, history, analysis), syncing, backing up and restoring device libraries.
2. **Link Export ("LINK EXPORT").** rekordbox joins the Pro DJ Link Ethernet network as a peer, takes a player number, and serves its own collection to CDJ/XDJ players as if it were a USB stick: it runs an NFS server for files and a remote database server for browsing and metadata, and exchanges status, beat and sync-master packets.
3. **Hardware settings.** Editing and pushing "My Settings" (player preferences), DJM mixer settings and device display settings, either as files on media or live over the network.
4. **Performance-mode hardware control.** Using CDJ/XDJ/OPUS/OMNIS units as USB HID (and LAN HID) controllers of rekordbox's own decks, including authenticated HID handshakes and pushing waveforms, artwork and text back to the players' screens.

Related but owned by other subsystems: cloud library sync and CloudDirectPlay (cloud subsystem), KUVO/rb-share cloud analysis (`NetworkAnalyze`, `NetAnalysis`), PRO DJ LINK Lighting (lighting subsystem, consumes this subsystem's link state), Ableton Link (sync subsystem), MIDI controller mappings (control subsystem).

## 2. Feature inventory (parity checklist)

### 2.1 Export mode and device detection

- Application has an EXPORT mode distinct from PERFORMANCE/LIGHTING/EDIT; switching modes unloads decks and is blocked while Pro DJ Link is active. `"Change EXPORT/PERFORMANCE/LIGHTING/EDIT mode"`, `"Are you sure you want to quit PRO DJ LINK?"`, `"You can not switch to the Performance mode while your computer is connected to Pro DJ Link."`
- Export mode layouts: 1 PLAYER, 2 PLAYER, DUAL PLAYER, SIMPLE PLAYER and browse-only (recommended for Link Export). `ExportModeLayoutSelector`, `"You can choose to display only the browse area. This layout is useful if using LINK EXPORT."`
- Automatic detection of mounted removable drives (USB, SD), listing in the "Devices" pane of the tree, eject button per device, alert when a drive is the system root or has an unsupported file system. `RemovableDeviceDetector::{getConnectedDrives,ejectDevice,isNotSupportedFilesystem,isSystemRootDevice}`, `browse::DriveList`, `browse::DeviceDriveSorter`, `db::DatabaseMediator::{execDeviceChange,construct_device_list,isNotSupportedDevice,isSystemRootDevice}`
- Writability and free-space checks before export; warning when a file system is unsupported by the target player (FAT16/FAT32 advice, exFAT only on some players). `check_device_write_access`, `getRemainingSize`, `getDriveFileSystemType`, `"Please format it to FAT16 or FAT32."`, `"DJ equipment that supports exFAT."`, `"Not enough space available on the device"`
- Device name and device background colour editable per device; shown on players. `modifyDeviceName`, `DevModifyDevName`, `"Device background color"`, `RekordboxExportSettingFile::{getBackgroundColor,setBackgroundColor}`
- Quit guard while writing: `"Writing to the device. Do you want to stop writing and quit the application?"`; `isDeviceDBModifing`, `isPossibleStopWritingDevice`.
- Unsupported device notice. `NotSupportedDeviceWindow`

### 2.2 Exporting to USB/SD (device library)

- Export a track, a selection, a playlist, a playlist folder (recursively) or the whole collection to a device by drag and drop or context menu. `db::DatabaseMediator::{exportTrack,exportPlaylist,exportPlaylistFolder,exportPlaylistRecursively,exportCollectionList,exportCollectionTrack,execExport}`, `db::DBCommandExport`
- Export writes, per track: the music file under `/Contents/<Artist>/<Album>/` (fallback `UnknownArtist/UnknownAlbum`), ANLZ files under `PIONEER/USBANLZ/`, artwork under `PIONEER/Artwork/` (with a `_m.jpg` small variant **(inferred)**), and rows in `export.pdb`/`exportExt.pdb` and/or `exportLibrary.db`. `calc_device_file_path`, `calc_device_analysis_path`, `export_analysis_data`, `export_ext_analysis_data`, `export_artwork`, `export_zoom_wave`, `"/Contents/UnknownArtist/UnknownAlbum/"`, `"PIONEER/USBANLZ"`, `"PIONEER/Artwork"`, `"_m.jpg"`, `create320x320Artwork`
- Choice of library format(s) written: legacy "Device Library", "Device Library Plus"/OneLibrary, or both kept identical. Setting `kDeviceDBType`; strings `"Export standard library"`, `"Export library plus"`, `"DEVICE-LIBRARY"`, `"DEVICE-LIBRARY-PLUS"`, `"Make Device Library and OneLibrary the same"`, `"Overwrite export playlists from rekordbox so that playlists in Device Library and OneLibrary are in the same state."`
- Convert an existing device library to OneLibrary from the tree context menu, with overwrite confirmation and step-by-step guide dialog. `DeviceLibraryPlusDlgManager::{onAddDevice,onUpdateDevice,onExecuteDeviceLibraryConv,toSqliteDbConversion,onDeviceLibraryPlusGuide}`, `ConvertDeviceLibraryPlusDlgMain`, `deviceSqliteDbConversion`, `isNeedDeviceSqliteDbConversion`, `"Convert to OneLibrary"`, `"OneLibrary already exists in the device. Do you want to overwrite it?"`
- "Match Device Library": map device tracks to collection tracks (matching help dialog). `DeviceLibraryMatchingHelpDlg`, `browse::DbMatchingButton`, `getMatchingTrackIDs`
- Re-export overwrite prompts for existing playlist/folder/list with the same name. `"Do you want to replace this list with the one you're exporting?"`, `overwrite_export`, `check_same_list_name_recursively`
- Format restrictions with export log: no FLAC/ALAC/streaming/video/Drum Capture on legacy targets; WAV/AIFF only at 44.1/48 kHz (96 kHz on hi-res capable players); separate "Export Hires" option. `"Export is not available when using the track formats below:"`, `askExportHiResoTrack`, `"Export Hires"`, `check_support_export`, `showExpotWarning`
- Per-playlist track limits: 1000 (legacy hardware) or 10000; max 1000 lists and folders per USB. `"Up to 1000 tracks can be added in one playlist"`, `"Up to 10000 tracks can be added in one playlist"`, `"The number of lists and folders cannot exceed 1000 on one USB device."`, `show_device_playlist_restriction`, `get_device_playlist_track_num_max`
- Export cancel and partial-failure report listing reasons (path > 255 chars, unsupported format, corrupt analysis, device error). `hasExportPlaylistCanceled`, `"One or more files were not exported. "`, `"Track(s) were not exported due to the following possible reasons."`
- Tracks are analysed (BPM/grid at least) on export if not yet analysed. `"If tracks are not analyzed, BPM/Grid analysis starts when importing, loading, or exporting the tracks..."`, `pursueAnalysis`
- Hot Cue Bank Lists exported as their own tree (list of cue references). `execExportHCBLCue`, `DevAddHotCueBanklist`, `DevAddTrackToHotCueBanklist`, `"HOT CUE BANK LIST Error"`
- My Tag export and "sync with My Tag" choice when the device is owned by another computer. `syncMyTag`, `syncMyTagWithAllDevices`, `"My Tag in this device is synced with another computer."`, `"Sync with My Tag"`
- Hot cue count rule: 8 hot cues (A to H) in EXPORT mode vs 16 elsewhere; hot cue colour type and auto-load flag exported. `"*8 Hot Cues can be set  to A to H in EXPORT mode..."`, `kHotCueMaxNum`, `kHotCueColorType`, `DevModifyHotCueAutoLoad`, `"Allow to auto load HotCue on CDJ/XDJ"`
- Beat grid export option and beat-jump defaults. `getExportBeatGrid`, `"Export BeatGrid information"`, `kExportBeatJump`
- Device menus (categories, sort columns, colour labels, menu items) written from rekordbox preferences so players show the user's chosen browse categories. `DevInitCategoryTable`, `DevInitMenuItemsTable`, `DevInitSortTable`, `DevEnableCategory`, `DevMoveSort`, `getCategorySetting`, `selectDeviceColor`, strings `"ARTIST"`, `"DJ PLAY COUNT"`, `"MATCHING"`, `"HOT CUE BANK"`
- Content filter on device (RBFLTR.DAT). `RekordboxExportSettingFile::{getContentFilter,setContentFilter,existsContentFilterSetting}`, `"RBFLTR.DAT"`, `PSvDBMain::GetFilterConditionForPlayer`
- Player log-out time for the device. `kDeviceLogOutTime`, `RekordboxExportSettingFile::{getLogOutTime,setLogOutTime}`
- Auto re-export of edited tracks/playlists to attached devices, and "Export tracks at the same time when importing them from an external drive". `kDeviceAutoReExport`, `DetailAutoExport`, `"Auto export"`
- Deleting tracks from a device playlist optionally deletes the files. `kDeviceDeletePlaylistTracks`, `rekordbox::DeleteDevicePlaylistTrackAlert`, `delete_device_song_file`, `delete_device_analysis_files`, `"Are you sure you want to delete track(s) in the playlist of the device?"`
- Editing device-side metadata in rekordbox (title, rating, colour, comment, cues, BPM) writes through to the device DB. `DevModify*` family (about 40 functions), `db::DevSqliteDBController::modify*`
- Garbage collection of orphaned rows and files on the device. `device_song_table_garbage_collection`, `delete_unused_record`, `deleteNonDeletedFiles`
- Corrupt device library detection with offer to delete. `"Device library is corrupted."`, `"Do you want to delete the device library ?"`, `deleteDeviceDBFile`

### 2.3 Sync Manager (playlist sync to device)

- Sync Manager window: pick rekordbox playlists (and iTunes playlists) to keep mirrored on a device; sync on demand or on attach; overwrite option. `PlaylistSyncManager::{syncDeviceIfNeeded,updateDeviceSyncSetting,createDeviceSyncSetting,syncItunesIfNeeded,mergeDevLibClassicToDevLibPlus}`, `PlaylistSyncManager::DbProxy`, `"Sync manager"`, `"Synchronize playlists with a device"`, `OverwriteSyncList`
- Sync state persisted on the device in XML side files: `/rekordbox/sync.temp`, `/rekordbox/sync.devlibplus.temp`, `/rekordbox/sync.itunes.temp`, plus master side `masterPlaylists6.xml` (`masterPlaylists3.xml` legacy). `DeviceSyncSetting::{read,write,defaultFileName,defaultPlusFileName,getDevicePlaylistIdToSync}`, `MasterSyncSetting`, `ItunesSyncSetting`
- Synchronisation method: selected files only vs all music files. `"Synchronize selected music files only"`, `"Synchronize all music files"`
- "Same device DB" mode where several playlists share one device DB **(inferred)**. `ExportDescription::setIsSameDevDbExport`, `getSameDeviceDBList`, `createListWithSameDevDbList`
- Device settings synced into Device Library Plus. `syncDevSettingsForDevLibPlus`

### 2.4 Reading devices back

- Browse a device's library inside rekordbox (legacy and Plus), play tracks from it in EXPORT mode, search it. `db::DevSqliteDBController::get*`, `PSvDBMain`/DeviceSQL `Dev*` readers, `"Can't load track on USB memory/SD card."`
- Import device playlists into the collection under `PioneerDJ/Imported from Device`, with options to copy files, analysis, artwork (artwork may be re-extracted from ID3), and a report window. `importDevicePlaylist`, `importDevicePlaylistLegacy`, `importTrackFromDevice`, `db::DatabaseMediator::importFromDeviceThread::{import_device_analysis,import_device_ex_track_data,import_from_d_file_copy,add_track_artwork}`, `importFromDeviceCopySettingWindow`, `importFromDeviceReportWindow`
- Import play history recorded by players from the device; dedupe by hash; `ImportedHistory.xml` on device; refuse to delete history used by Link Export. `importHistoryFromDevice`, `DeviceHistory::ImportedHistoryManager::{add,exist,getHash,serializeToXml}`, `"/rekordbox/ImportedHistory.xml"`, `"IMPORTED-HISTORY"`, `"Delete history from device"`
- Track information replace-from-device (colour, rating, comments). `"The following information of applicable tracks in rekordbox will be replaced with that of all the tracks in the device."`
- Collection Radar/Streaming Radar is unavailable for USB-exported tracks. `"Collection Radar/Streaming Radar cannot be run for tracks exported from a USB memory."`

### 2.5 Device library backup and restore

- Backup a device library to the computer: manual "Backup now" and "Automatically backup device library upon removal"; selectable backup destination. `backupDeviceLibrary`, `kDeviceLibBackup`, `kDeviceLibBackupDest`, `db::LibraryBackupThread`, `db::BackupZipBuilder`, `"DeviceLibBackup"`, `"rbDevLibBa_"`, `"rbDevLibBaInfo_"`, `"backup_generation"`
- "Backed up device library management" window: table of backups (device name, date/time), restore selected backup to a device (choose destination file system), delete backups. `BackupedDeviceLibDialog`, `BackupedDeviceLibTable::{restoreSelectedBackup,deleteSelectedBackup,sortOrderChanged}`, `DeleteDeviceLibBackupWindow`, `restoreDeviceLibrary`, `existsDeviceLibBackup`, `"Restorable device library"`, `"Restore destination file system"`
- Restore reports missing tracks and backup errors. `DeviceLibRestoreMissingTrackDialog`, `DeviceLibBackupErrorComponent`
- Cloud backup of device library (cloud subsystem, uses the same data). `CloudDeviceLibraryBackup`, `DeleteDeviceLibBackupWindow::run` (HTTP callbacks)
- Delete device library list UI. `DeviceLibDeleteComponent`, `DeviceLibDeleteList`

### 2.6 Player and mixer settings files

- "My Settings" editor for CDJ/XDJ player preferences, saved locally and exported to `PIONEER/MYSETTING.DAT` and `PIONEER/MYSETTING2.DAT`. Fields: auto cue on/level, beat jump beat value, disc slot brightness, eject/load lock, play mode (single/continue), gate cue, hot cue auto load, hot cue colour, jog display mode, jog LCD brightness, jog mode (vinyl/CDJ), jog ring brightness and flashing, language, LCD brightness, master tempo, needle lock, on-air display, overwrite current cue with hot cue, pad button brightness, phase meter style, quantize on and beat value, slip flashing, sync, tempo range, time mode (remain/elapsed), vinyl speed adjust, waveform divisions, waveform phase meter. `pioneerMySetting::MySettingData::{get,set}*`, `pioneerMySetting::MySettingFile::{saveDeviceFile,loadDeviceFile,saveExtFile,loadExtFile,exportToDeviceFromLocalFile}`, `MySetting*Component` (40 UI components)
- DJM mixer My Settings saved to `PIONEER/DJMMYSETTING.DAT`, including channel fader curve, crossfader curve, headphone pre-EQ and mono split, mic low cut, talk-over mode/level, beat FX quantise, MIDI channel/button type, plus a separate BFX section **(inferred from component names)**. `pioneerMySetting::DJMMySettingFile`, `DJMMSContent`, `DJMMSData`, `DJMMSDataBFX`, `exportDJMMySettingFileToDevice`, `MySettingCHFaderCurveComponent`, `MySettingTalkOverModeComponent`
- Device display settings saved to `PIONEER/DEVSETTING.DAT`: waveform colour (blue/RGB/3-band), waveform current position, waveform display style, waveform divisions, image on jog display, show user name on screen, key display style. `cdjDeviceSetting::DeviceSettingData::{get,set}*`, `cdjDeviceSetting::DeviceSettingFile`, `"Waveform color displayed on CDJ/XDJ"`, `"Image displayed on the CDJ/XDJ jog display"`
- Settings files carry a CRC and vendor/product name strings. `cdjDeviceSetting::calcCRC`, `pioneerMySetting::calcCRC`, `s_vendorName`, `s_productName`
- "Apply settings on devices automatically" and "Apply on all connected devices"; delete settings file on device. `kDeviceApplyMySettingOnDevices`, `"Do you want to apply mySetting to all connected devices ?"`, `"Do you want to delete My Settings file in the device?"`, `MySettingFile::deleteFile`
- Remote setting over Pro DJ Link: push My Settings and DJM settings to linked units, refused while a player is playing. `djplay::RemoteSettingController::{sendMySettingsToPlayer,sendDJMMySetting}`, `"Remote setting using PRO DJ LINK"`, `"Cannot overwrite the setting of the CDJ/XDJ on PLAY status."`

### 2.7 Link Export (rekordbox as a Pro DJ Link peer and media server)

- Enable/disable link from the UI; link status widget shows each linked player and mixer (player number, model, playing/paused, master, sync, on-air, MIDI/HID icon, loaded track title). `djplay::WidgetProDJLink`, `djplay::WidgetLinkPlayer`, `WidgetLinkMixer`, `WidgetLinkXdj`, `WidgetLinkXdjRx`, `WidgetLinkXdjRx2`, `WidgetLinkOpusQuad`, `WidgetLinkMasterButton`, `WidgetLinkMasterBPM`, `WidgetLinkStatusBarCaution`, `"Link On"`, `"Link must be activated"`
- Configurable link export name. `kLinkExportName`
- Network interface selection and link up/down tracking (IP change, BSSID change, cable disconnect). `PSvLinkSysMgrNetworkAccess::{getLinkIF,getMyIPAddr,getMyMACAddr,getMyNetMask,getSsidAndBssid,isIpChanged,isNetDisconnect}`, `"%s Link DOWN bssidchange"`, `"%s Link DOWN ipchange"`
- Port availability check for UDP 50000/50001/50002/50004/50111 and NFS 2049 before starting; error if a port is busy. `"portchk"`, `"%s OK 50000"` ... `"%s OK 2049"`, `"%s NG ProDJLink Port can not use port = %d"`, `ProDJLink::noticePortOpenError`
- Device number claim and keep-alive on the link (rekordbox occupies a player slot). `SysMgrMainComponent::{frameSendDiscoveryRequest,frameSendIdSetRequest,frameSendIdUseRequest,readIdUseResponse,readConfigNotify,readReject,readCompatiRes}`, `PSvLinkKeepAlive`, `ProDJLink::getSelfLinkID`
- Compatibility gating: old CDJ-2000/CDJ-900 firmware blocked; XDJ access-point networks; refuse mixed CDJ and XDJ AP networks. `"%s NG CDJ-2000 CDJ-900 BLOCK linkif = %d frmhdr->minorver = %d model = %s"`, `"%s Connected XDJ AP network"`, `"%s NG CDJ AND XDJ EXIST ip = ..."`, `ProDJLink::{isCDJNetwork,isWlanNetwork}`
- Serve the collection to players: players browse rekordbox's library (playlists, categories, search, history, hot cue bank lists, My Tag filters) through the remote DB protocol and fetch audio through NFS. `PSvDBMain::On*Cmd` (album, artist, BPM, colour, genre, key, label, length, rating, year, history, playlist, My Tag, matching, search ...), `PSvDBServer`, `PSvNFSd`, `NFSdMainComponent::{addExportList,addNFSPermission,setPermittedFileExtensionList}`, `ProDJLink::requestSetPermittedFileExtensionList`
- Players request ANLZ data, artwork, waveforms and cue lists; rekordbox answers from its own analysis store. `PSvDBMain::{OnSongAnlzCmd,OnMAnlzClientCmd,OnImgCmd,GetWave,GetUsbCue,GetUsbCueExt,GetQtzInf,GetVbrInf,GetDBImgData}`
- Players write back: cue points set on a CDJ, BPM edits, rating, play count and history are saved into rekordbox. `PSvDBMain::{SavUsbCue,SavUsbCueExt,SavQtzOfs,SavWave,SavVbrInf,SetHCBnkCuePoint,OnWriteCmd,OnDbModCmd}`, `PSvMasterDBIF::{setCue,modTrackBPM,modTrackRate,insertSong2History,makeNewHistory}`, `UiProDJLink::{ReceiveSetCueEvent,ReceiveDeleteCueEvent,ReceiveDeleteHisotryCmd}`
- "Coloring of played tracks" for tracks played via Link Export, with reset on exit option. `clearAllLinkPlayedStatus`, `getAllLinkPlayed`, `"Coloring of played tracks "`, `"Reset color when exiting rekordbox"`
- Load a track from rekordbox to a specific player ("Load track to player 1..6") by drag and drop onto the link widget or command; refused for streaming tracks and for players with loading disabled. `ProDJLink::{dragDropPlayMusic,responseDragDropPlayMusic}`, `InnerLinkAPI::musicDragDropResponse`, `"Load track to player 1"`, `"Invalid Operation. Streaming tracks can not be loaded to CDJ/XDJ."`, `"Loading is currently disabled for the specified device."`
- Drag a Hot Cue Bank List to a player. `ProDJLink::dragDropHCueBankList`, `PSvLinkHotCueBankReqInfo`
- Broadcast "renew" notifications when the library, cues, export list or device settings change so players refresh. `LinkProxy::{broadcastCdjRenew,broadcastCueRenew,broadcastExportListRenew,broadcastDevsettingRenew}`, `PSvDBServer::NotifyLibraryUpdated`
- Track the state of all linked units: BPM, beat position, on-air, play state, sync state, which content each player has loaded, whether it is playing a track from our collection. `prodjlink::LinkDeviceManager::{updateLinkDeviceBPM,updateLinkDeviceBeatPos,updateLinkDeviceOnAir,updateLinkDeviceSyncStatus,setPlayingContentsData}`, `ProDJLink::{isLoadingByMyCollection,getLoadingContentID}`
- Tempo master participation: rekordbox can become/yield sync master and send its BPM and beat packets **(inferred: when a rekordbox deck is master)**. `ProDJLink::{setBeatSyncMaster,setCurrentBPM,sendSyncMasterRes,receiveSyncMasterReq,noticeWillChangeSyncMaster,noticeDidChangeSyncMaster}`, `PSvLinkSyncMasterReqSndInfo`, `PSvLinkBeatSndInfo`, `PSvLinkTimeBeatCounter`
- Link monitor (headphone cue of a linked player's audio through rekordbox) **(inferred from name and "LINK CUE")**. `ProDJLink::{enableLinkMonitor,disableLinkMonitor,GetLinkmonitorData}`, `PSvLinkMonitor`, `DJSystemLinkMonitor`, `kLinkUseLinkMonitor`, `"LINK MONITOR"`, `"LINK CUE"`
- Multi-waveform view request to players (CDJ-3000 multi-deck waveform) **(inferred)**. `LinkProxy::unicastMultiWaveViewReq`, `PSvLinkMultiWaveViewReqInfo`
- Song structure (phrase) data exchange for PRO DJ LINK Lighting. `ProDJLink::receiveDeviceSongStructRes`, `UiProDJLink::ReceiveDeviceSongStruct`, `djplay::CDJLightingLinkManager`
- Players' browse hardware (back button, rotary selector) relayed to rekordbox ("SDP" messages) for a CDJ-style browse tree mirrored in rekordbox **(inferred)**. `ProDJLink::{receiveSdpBackButtonStatus,receiveSdpRotaryButtonStatus,receiveSdpRotarySelectorStatus}`, `browse::CdjDispTree::{eventDial,eventBack,eventForward}`
- Privacy: hide track information of tracks played on CDJ/XDJ from apps connected via Pro DJ Link. `"Do not publish track information "`, `"The information of the tracks played on CDJ/XDJ can be hidden on the app connected with Pro DJ Link."`
- Feature locks while linked: no beat grid edits, no EDIT mode, no Lighting mode (unless PRO DJ LINK Lighting), no Cloud Library Sync. `"Beat grid adjustment is allowed only while PRO DJ LINK is not activated."`, `"Cloud Library Sync cannot be started while using LINK EXPORT."`
- Mixer version query and DJS-1000 special case. `PSvLinkMixerVersionReq`, `"DJS-1000"`

### 2.8 Mobile and Wi-Fi library sync (rekordbox for iOS/Android, "RBM")

- Search for mobile devices on the network, connect/disconnect, show memory, OS (iOS/Android), name. `ProDJLink::{broadcastDeviceSearch2,stopDeviceSearch2,getMobileDeviceName,getMobileDeviceMemory,isMobileDeviceiOS,isMobileDeviceAndroid}`, `prodjlink::MobileDeviceManager`, `djplay::DetectedRbmList`, `"SEARCH MOBILE button"`
- Wi-Fi Library Sync: transfer tracks to/from mobile, delete from mobile, progress counts (transfer/forwarding/delete totals and remaining). `UiProDJLink::{requestMobileTransferTotalCount,requestMobileDeleteRemainCount,...}`, `LinkProxy::{unicastSendTrackFilePath,unicastGetTrackFileReq,unicastDeleteTrackFileReq,unicastLibrarySyncStartReq,unicastGetMobileTrackListReq}`, `"Wi-Fi Library Sync"`, `"Songs transferred to mobile"`
- Mutually exclusive with Link Export and cloud sync. `"LINK EXPORT cannot be used while connected to a mobile device via Wi-Fi."`
- RBM authentication and settings round-trips (get/send device settings and My Settings to mobile). `ProDJLink::receiveRBMAuthentication`, `requestRBMSendDevSetting`, `requestRBMGetMySetting`, `"requestRBMSendFile("`

### 2.9 Performance-mode HID control of players

- Connect CDJ/XDJ/OPUS/OMNIS/DJM units by USB as HID controllers of rekordbox decks; also "LAN-HID" over the network for some models. `djplay::DeviceManager::{connectHidDevice,connectLanHidDevice}`, `djplay::HidMapLANHID`, `"### LAN-HID:CDJ:[%s] enableDevice"`, `"USB or HID"`
- Per-model HID maps: CDJ-350, 400, 850, 900, 900NXS, 2000, 2000NXS2, 1500X, 3000, 3000X; XDJ-1000, 1000MK2, RX, RX2, RX3, XZ, AZ, AN; OPUS-QUAD; OMNIS-DUO; DJM-900NXS2, DJM-S11; DDJ-1000, FLX10, REV7 and others; "Sub" maps for second-deck/second-layer reports. `djplay::HidMapCDJ3000` etc. (see class list)
- HID authentication handshake (challenge/response) before a unit accepts control; product info request (vendor, product ID, firmware, serial). `djplay::DeviceAuth::{sendChallengeA,recvResponseA,sendResponseE,makeHash,makePassword}`, `djplay::DeviceAuth2`, `djplay::ProductInfoRequest`, `djplay::DeviceProductInformationFormat`, `HidMap::{getDeviceAuth,getHidAuthFormat,inputUnitAuth}`
- Deck assignment (which rekordbox deck a CDJ controls; DECK 1/3, 2/4 on XDJ/OPUS). `"CDJ DECK Set %d"`, `"XDJ DECK Set %d"`, `inputUnitForDeckSelect`
- Display feedback pushed to player screens: song info, browsing lists, album art (normal and large), whole and detailed waveforms (blue, colour, 3-band), beat grid, cue and hot cue points, pad info, key, BPM, countdown, Mix Point Link state, vocal position, stem separation progress. `djplay::DisplayDataManager`, `djplay::ControllerDisplay2Builder::setFieldMapForCDJ3000` etc., `*Builder` classes, `HidMapCDJ3000::on*Changed`
- Browse a CDJ-style tree on the player screen. `browse::CdjDispTree`, `browse::CdjDispItem`
- HID report plumbing on macOS via IOKit run loop; output priority queue. `HidRunLoopClass`, `djplay::MAC_HidInput`, `djplay::PdjUsbHidOutputHelper`, `djplay::HidReportRingBuffer`
- KUVO display/HID path for club info (DJ name, now playing) **(inferred)**. `djplay::HidMapKUVO`, `HidKuvoManager`

### 2.10 Other device features

- NXS-GW (KUVO gateway hardware) detection and track reporting. `NxsGatewayClient::{reqGatewayAddress,reqDjKey,sendTrackData,checkInternetConnection}`, `"NXS-GW detected"`
- DJ device usage log collection from hardware **(inferred)**. `DJDeviceUsageLogManager`
- Resident firmware update notifier and setting-utility update prompts. `ResidentFirmwareUpdateManager`, `SettingUtilityUpdateWindow`
- Setup utility for audio interfaces of mixers/players (USB input routing, mixer output, thru/phono). `PDJSetupUtility`, `PDJSetupUtilityDJMV10`
- CloudDirectPlay device authentication of USB sticks so compatible players can log in (owned by cloud subsystem, touches device). `CloudDJDeviceAuthenticationComponent`, `"Authentication USB device login"`
- OSC "DJ monitor" status sender for third-party apps **(inferred)**. `djplay::djmonitor::DJStatusSender::sendOSCMessage`, `"Third-party Application Linkage"`

## 3. Data formats and protocols

### 3.1 Device folder layout

Observed paths (root = volume root; some players use a hidden `.PIONEER`):

| Path | Content | Evidence |
|---|---|---|
| `PIONEER/rekordbox/export.pdb` (or `.PIONEER/rekordbox/export.pdb`) | Legacy DeviceSQL device library | `"/PIONEER/rekordbox/export.pdb"`, `"/.PIONEER/rekordbox/export.pdb"` |
| `PIONEER/rekordbox/exportExt.pdb` | Extension DeviceSQL DB (My Tag, cue options, property options, history extras) | `"/PIONEER/rekordbox/exportExt.pdb"`, column groups `djdbex*` |
| `PIONEER/rekordbox/exportLibrary.db` | Device Library Plus / OneLibrary, SQLCipher | `"exportLibrary.db"`, `libsqlcipher.0.dylib` |
| `PIONEER/USBANLZ/Pnnn/xxxxxxxx/ANLZ0000.{DAT,EXT,2EX}` | Per-track analysis | `"PIONEER/USBANLZ"`, `"ANLZ0000.DAT"`, `"ANLZ0000.EXT"`; `.2EX` from prior art and `pmai_ex2` methods |
| `PIONEER/Artwork/...` | Artwork JPEGs, small and `_m` variants | `"PIONEER/Artwork"`, `"_m.jpg"`, `calc_next_device_image_path` |
| `PIONEER/MYSETTING.DAT`, `MYSETTING2.DAT` | Player My Settings | strings |
| `PIONEER/DJMMYSETTING.DAT` | Mixer My Settings | string |
| `PIONEER/DEVSETTING.DAT` | Device display settings | string |
| `PIONEER/rekordbox/...` RBFLTR.DAT **(location inferred)** | Content filter | `"RBFLTR.DAT"` |
| `Contents/<Artist>/<Album>/<file>` | Audio files | `"/Contents/"`, `"UnknownArtist/"` |
| `rekordbox/ImportedHistory.xml`, `rekordbox/sync*.temp`, `rekordbox/playinfo`, `rekordbox/request` **(purpose of last two unknown)** | Side files | strings |
| `PIONEER/Master/` | Master DB folder when a library lives on a drive | `"PIONEER/Master/"` |

The path hashing scheme for `USBANLZ` (`Pnnn/xxxxxxxx`) is documented by crate-digger **(prior art)**; the binary contains `"/ANLZ%04X.DAT"` and `"%05d.DAT"`, so at least one variant uses hex-numbered file names. Confirm against a real export.

### 3.2 export.pdb / exportExt.pdb (DeviceSQL)

- rekordbox embeds a compiled Encirq/Ubiquity **DeviceSQL** runtime (`edb_*` symbols, about 170) and generated query modules: `DEV_MODULE1..9` for the device DB and `MAS_MODULE1..7` for the legacy rekordbox 5 master DB (`datafile.edb`, `LocalData.edb`, `ExtData.edb`). Each module has `init_forms`, `reset_forms`, `clean_forms`, `table_inits`.
- Logical tables seen as identifiers: `djdbContent`, `djdbGenre`, `djdbArtist`, `djdbAlbum`, `djdbLabel`, `djdbKey`, `djdbColor`, `djdbPlaylist`, `djdbSongPlaylist`, `djdbHotCueBanklist`, `djdbSongHotCueBanklist`, `djdbHistory`, `djdbSongHistory`, `djdbImage`, `djdbHotCueBankPoint`, `djdbMenuItems`, `djdbProperty`, `djdbCategory`, `djdbSort`; extension tables `djdbexHistory`, `djdbexCueOption`, `djdbexDBPropertyOption`, `djdbexMyTag`, `djdbexSongMyTag`.
- Content columns (subset): `SearchStr`, `Subtitle`, `Length`, `DiscNo`, `ArtistID`, `RemixerID`, `OrgArtistID`, `ComposerID`, `AlbumID`, `GenreID`, `LabelID`, `ImageID`, `ReleaseYear`, `ReleaseDate`, `DateCreated`, `StockDate`, `FolderPath`, `FileNameL`, `FileSize`, `FileType`, `BitRate`, `BitDepth`, `SampleRate`, `DjPlayCount`, `MasterDBID`, `MasterContentID`, `AnalysisDataPath`, `Analysed`, `ContentLink`, `reserved1..9`. Cue options columns: `BeatSize`, `CueMicro`, `InSeekInfo`, `OutSeekInfo`, `InMsec`, `OutMsec`, `InFrame`, `OutFrame`, `InMpegFrame`, `OutMpegFrame`, `InMpegAbs`, `OutMpegAbs`. Property: `DevName`, `SongNumber`, `MyTagMasterDBID`. Recommended-like: `ContentID1`, `ContentID2`, `LikeRate`.
- On-disk page format: not visible from symbols (the runtime is generic). Public Kaitai spec `rekordbox_pdb.ksy` in crate-digger covers: file header with page size (4096), table list (type, empty candidate, first/last page), data pages with heap and row-index groups of 16 with presence bitmasks, `DeviceSQLString` encodings (short ASCII, long ASCII, UTF-16LE) **(prior art)**. Writing parity needs the unknowns in section 5.
- The legacy DB is written through the DeviceSQL runtime (`DevAddContent`, `DevAddPlaylist`, `DevSetAnalysisData`, ...), not by hand-rolled code **(inferred)**, so row placement, free-space reuse and index ordering follow DeviceSQL rules that crate-digger only partly documents.

### 3.3 exportLibrary.db (Device Library Plus / OneLibrary)

- SQLite with SQLCipher encryption (`libsqlcipher.0.dylib` bundled; `db::DevSqliteDBController::_dbPwd`). The key material is in the binary in obfuscated form next to the `"exportLibrary.db"` string; **not recorded here**. pyrekordbox documents how to open it **(prior art)**.
- Pragmas: `journal_mode = WAL`, `synchronous = NORMAL`.
- Tables (from `DevDBTables` and identifiers): `album`, `artist`, `category`, `color`, `content`, `cue`, `genre`, `history`, `history_content`, `hotCueBankList`, `hotCueBankList_cue`, `image`, `key`, `label`, `menuItem`, `myTag`, `myTag_content`, `playlist`, `playlist_content`, `property`, `recommendedLike`, `sort`.
- Column names observed (camelCase, `*_id` foreign keys): `titleForSearch`, `subtitle`, `bpmx100`, `trackNo`, `discNo`, `artist_id_artist`, `artist_id_remixer`, `artist_id_originalArtist`, `artist_id_composer`, `artist_id_lyricist`, `album_id`, `genre_id`, `label_id`, `key_id`, `color_id`, `image_id`, `djComment`, `releaseYear`, `releaseDate`, `dateCreated`, `dateAdded`, `fileName`, `fileSize`, `bitDepth`, `samplingRate`, `djPlayCount`, `isHotCueAutoLoadOn`, `isKuvoDeliverStatusOn`, `kuvoDeliveryComment`, `masterDbId`, `masterContentId`, `analysisDataFilePath`, `analysedBits`, `contentLink`, `hasModified`, `cueUpdateCount`, `analysisDataUpdateCount`, `informationUpdateCount`, `nameForSearch`, `isComplation` (sic), `sequenceNo`, `playlist_id_parent`, `colorTableIndex`, `cueComment`, `isActiveLoop`, `beatLoopNumerator`, `beatLoopDenominator`, `inUsec`, `outUsec`, `in150FramePerSec`, `out150FramePerSec`, `inMpegFrameNumber`, `outMpegFrameNumber`, `inMpegAbs`, `outMpegAbs`, `inDecodingStartFramePosition`, `outDecodingStartFramePosition`, `inFileOffsetInBlock`, `OutFileOffsetInBlock`, `inNumberOfSampleInBlock`, `outNumberOfSampleInBlock`, `isVisible`, `isSelectedAsSubColumn`, `deviceName`, `dbVersion`, `numberOfContents`, `createdDate`, `backGroundColorType`, `myTagMasterDBID`, `content_id_1`, `content_id_2`. Indexes: `index_playlist_content_playlist_id`, `index_hotCueBankList_cue_hotCueBankList_id`, `index_myTag_content_myTag_id`, `index_myTag_content_content_id`.
- Exact DDL (types, defaults, constraints) is assembled at runtime (`createTables`, `createIndexes`); get it from a real file or decompilation.
- Cue rows carry precomputed seek information (MPEG frame numbers, decoding start frame, file offset in block, samples in block) so players can seek without decoding. Computing these needs the audio decoder (`MpegFrameOffset`, `GetSeekPointAtTime`).
- OneLibrary is the public name of this format since October 2025, shared with Algoriddim djay and Native Instruments Traktor; VirtualDJ also supports it. Compatible players listed publicly: CDJ-3000/3000X, OMNIS-DUO, OPUS-QUAD, XDJ-AZ (**prior art / press**). In 7.2.19 strings it is offered as "Convert to OneLibrary".

### 3.4 ANLZ analysis files

- Writer/reader: `analyzer::CMusicAnalyzeFile`. Three containers, all with a `PMAI` header: `.DAT` (`pmai`), `.EXT` (`pmai_ext`), `.2EX` (`pmai_ex2`). Big-endian integer helpers `Read8..64`/`Write8..64`.
- Sections written to `.DAT`: path (`PPTH`), VBR seek index (`PVBR`), quantise/beat grid (`PQTZ`), memory and hot cue lists (`PCOB`), preview waveform 400 points and tiny 100 point waveform (`PWAV`, `PWV2`). Evidence: `write_pmai_path`, `write_pmai_vbr`, `write_pmai_quantize`, `write_pmai_hot_cue`, `write_pmai_memory_cue`, `write_pmai_wave400pt`, `write_pmai_wave100pt`, string `"PPTHPVBR"`.
- `.EXT`: extended cues with colour, comment, loop length, colour table index (`PCO2`), path, new-format whole and zoom colour waveforms (`PWV3`, `PWV4`, `PWV5`), extended quantise (`PQT2`), seek table, song structure/phrases (`PSSI`), beat analysis type and offsets. Evidence: `write_pmai_ext_cue_ext`, `write_pmai_ext_new_whole_wave`, `write_pmai_ext_new_zoom_wave`, `write_pmai_ext_quantize_ext`, `write_pmai_ext_seek_table`, `write_pmai_ext_song_struct`, `read_pmai_ext_unknown_atom` (unknown tags are preserved).
- `.2EX`: 3-band whole and zoom waveforms (`PWV6`, `PWV7`), waveform normalise coefficient (`PWVC`), and **vocal detection data with a version** (`write_pmai_ex2_vocal_detect`, `set_pmai_ex2_vocal_detect_version`), which is newer than public specs.
- Song structure section in `.EXT` is XOR-masked in files written by rekordbox 6+ **(prior art, crate-digger)**; confirm which variant 7.2.19 writes.
- Tag four-character codes come from crate-digger `rekordbox_anlz.ksy` **(prior art)**; the binary stores them as integers, so map each writer method to its tag by comparing a real export.
- Artwork: JPEG, a 320x320 variant is generated (`create320x320Artwork`); image IDs allocated sequentially (`get_next_image_id`).

### 3.5 Settings files

- `MYSETTING.DAT`, `MYSETTING2.DAT`, `DJMMYSETTING.DAT`, `DEVSETTING.DAT`: small binary files with a header carrying vendor and product name strings, a body ("data chunk") of enumerated byte fields, and a CRC (`calcCRC`). Field order and enumerations are documented in crate-digger's `rekordbox_settings` work and beat-link **(prior art)**. Local copies are kept in the rekordbox settings folder alongside `"DEVSETTING.DAT"`, `"DJMMYSETTING.DAT"`.
- DJM settings file has "created by rekordbox" detection, enable/disable flag, and merge semantics so unknown fields from newer mixers are kept (`isCreatedByRekordbox`, `overwriteWithMerge`, `getMinimumSize`).

### 3.6 Pro DJ Link network protocol

Ports confirmed by `"portchk"` strings in the binary: UDP 50000, 50001, 50002, 50004, 50111 and NFS 2049. Meanings below for 50000-50004 are **(prior art, Deep Symmetry DJ Link analysis)**:

| Port | Use | Packet types (byte after 10-byte header + device name) |
|---|---|---|
| UDP 50000 | Announcement, device number claim, keep-alive | `0a` hello, `00` claim stage 1, `02` claim stage 2, `04` claim stage 3, `06` keep-alive, `08` conflict, `01`/`03`/`05` mixer-assigned numbers |
| UDP 50001 | Beat and sync | `02` fader start, `03` channels on air, `0b` absolute position (CDJ-3000), `26` master handoff request, `27` handoff response, `28` beat, `2a` sync control |
| UDP 50002 | Status, media, remote | `05` media query, `06` media response, `0a` CDJ status, `19` load track, `1a` load ack, `29` mixer status, `34` load settings |
| UDP 50004 | Touch audio (CDJ-3000 preview) | `1e` audio data, `1f` handover, `20` timing |
| UDP 50111 | Unknown; possibly mobile (RBM) discovery **(inferred)** | |
| TCP 12523 | DB server port query: client sends the service name, server replies with a port | string `"RemoteDBServer"` in binary; port number from prior art |
| TCP (dynamic) | Remote DB ("dbserver") protocol | `PSvDBServer`, `PSvDBConnection::{SendItem,ReceiveItem}` |
| UDP 111, NFS 2049, mountd | ONC RPC portmapper, MOUNT and NFS v2 for file access | `PSvLinkPortMapperThread`, `PSvLinkMntdThread`, `PSvLinkNFSdThread`, `"%s OK 2049"` |
| TCP (rekordbox internal) | Link API message transport between rekordbox subsystems, and `/tmp/PSvLinkProc` IPC | `PSvLinkTCPServer`, `PSvLinkTCPClient`, `PSvLinkIPC`, `"/tmp/PSvLinkProc"` |

- Common header: 10-byte magic `Qspt1WmJOL` (present in binary), then type byte, device name (20 bytes, padded), and a sub-type/length layout per packet **(prior art)**. `PSvLinkFrameHeader::{checkEqual,checkUpper,getFrameSize,matchMessage}` and log text `frmhdr->minorver` show rekordbox parses a protocol minor version from the header.
- rekordbox-side message classes (each is one packet type; exact type bytes are open questions): device search (`PSvLinkDeviceSearch2Info`), device connect v1/v2 and disconnect (`PSvLinkDeviceConnect*`, `PSvLinkDeviceDisconnect*`), keep-alive (`PSvLinkKeepAlive`), CDJ renew (`PSvLinkCdjRenewInfo`), cue renew (`PSvLinkCueRenewInfo`), export list renew (`PSvLinkExportListRenewInfo`), device/My Settings read, write and renew (`PSvLinkDevSetting*`, `PSvLinkMySetting*`, `PSvLinkDJMMySetting::{ReadReq,ReadRes,WriteReq,DuplicateReq}`), sync master request/response (`PSvLinkSyncMaster*`), sync mode and status (`PSvLinkSyncModeInfo`, `PSvLinkSyncStatusInfo`), beat send/receive (`PSvLinkBeatSndInfo`, `PSvLinkBeatRcvInfo`), drag-and-drop load (`PSvLinkDDIndicationInfo`, `PSvLinkMusicDDResInfo`), hot cue bank request/response, NFS device property request/response and error (`PSvLinkNfsDevProp*`, `PSvLinkNfsErrorInfo`), file get/exist/delete (`PSvLinkFileGetReqInfo`, `PSvLinkFileExistInfo`, `PSvLinkDelFileInfo`), mobile track list/get/send/delete and library-sync status (`PSvLinkGetMobileTrackList*`, `PSvLinkGetTrackFile*`, `PSvLinkSendTrackFile*`, `PSvLinkDelTrackFile*`, `PSvLinkLibrarySyncStartReqInfo`, `PSvLinkGetLibrarySyncStatus*`), name query (`PSvLinkGetNameInfo`), link monitor data (`PSvLinkMonitorReqInfo`, `PSvLinkMonitorDataInfo`), multi waveform view (`PSvLinkMultiWaveView*`), mixer version (`PSvLinkMixerVersionReq`), player link info (`PSvLinkPlayerLinkInfo`), SDP HID/link info (`PSvLinkSdpHidInfo`, `PSvLinkSdpLinkInfo`), dev export start/end (`PSvLinkDevExportStart`, `PSvLinkDevExportEnd`).
- System manager state machine (`SysMgrMainComponent`): discovery request, ID set request/response, ID use request/response, ID block request, reject, config notify, compatibility response, disconnect, time server request; timers for discovery, ID response/retry, aging, random back-off and network monitor; "Mode23" suspend variant **(meaning unknown)**. These map onto the 50000 claim sequence **(inferred)**.
- Intervals: normal and short interval schedulers (`PSvLinkNormalInterval`, `PSvLinkShortInterval`), beat counter thread (`PSvLinkTimeBeatCounter`), timers `expTimer`, `expbeatInfoTimer`, `expmsTimer`, `expmsTimerSyncMaster`, `expmsTimerSyncSend` in `InnerLinkAPI`. Exact periods are open questions; prior art gives about 1.5 s keep-alive and per-beat beat packets.
- Feature probes per peer: `PSvLinkNormalInterval::{isSupportDeviceSetting,isSupportMysetting,isSupportDuplicationv2}`.

### 3.7 Remote DB ("dbserver") protocol

- rekordbox implements the server side (`PSvDBMain`, about 155 methods). Request families seen as handlers: root menu, sort menu, list by category (album, artist, bitrate, BPM, colour, genre, key, new key, label, length, rating, year, file name, play count, history, playlist, My Tag, matching), track list, song info, song analysis (ANLZ tag fetch), image (artwork), cue bank, list buffer paging (`GetListBuf1stRow`, `GetListBufRowContent`, `ListBufSort`), filter (`OnFilterCmd`), prepare/update prepare, write and DB-modify commands, system info.
- Wire format **(prior art, dysentery)**: messages begin with a 4-byte magic, then a transaction ID, a 2-byte message type and argument count, a 12-byte argument-type tag list, then typed fields (1/2/4-byte numbers, binary blobs, UTF-16BE strings). Menu responses are "menu header / menu items / menu footer" sequences. Request types include setup, root menu, metadata, artwork, preview and detail waveforms, beat grid, cue lists and generic ANLZ tag requests.
- Images are bounced to the main thread (`CMD_GET_IMAGE`/`CMD_RET_IMAGE` log strings) and size-checked (`CheckImgSize_XY`).
- `RetCueToClient` and `RetNewCueToClient` imply two cue response formats (legacy and extended-colour).
- Per-connection socket buffers `"rbuf=4096 wbuf=4096"`.

### 3.8 NFS export

- rekordbox runs its own user-space NFS stack in-process: portmapper, mountd and nfsd threads (`PSvLinkPortMapperThread`, `PSvLinkMntdThread`, `PSvLinkNFSdThread`) managed by `NFSdMainComponent` with an export list (`addExportList`, `delExportList`), per-client permissions (`addNFSPermission`) and a permitted file extension list.
- Players mount it as if rekordbox were a media slot. Prior art documents players as NFS v2 over UDP clients **(prior art)**.

### 3.9 USB HID control

- Per-model HID report maps with input units and output units (LED, display text, waveform blocks), SysEx-like output messages (`outputUnitSysEx`, `addSysEx`), blink handling.
- Authentication: challenge A sent by rekordbox, response A from device, response E back, using a hash with a vendor table and level **(inferred from method names; algorithm and table are secrets, not recorded)**.
- `HidExtentFormat` and `isOldHidFormat` indicate two report framings.
- The CDJ-3000 map receives full-resolution colour and 3-band waveforms, beat grid, cue points, large album art and KUVO text fields.

### 3.10 Helper executables

| Executable | What it is | Evidence |
|---|---|---|
| `edb_streamd` | DeviceSQL stream daemon: a small TCP/FIFO daemon (`EDB_LISTEN_PORT`, `EDB_FIFO_PATH`, `/tmp/edb_daemon_log`, lock and guard files in `/tmp`) that the DeviceSQL client library spawns; `PSvDBMain::executeStreamD` checks it is running before the DB server starts. | cstrings; `"PSvDBMain::Start:edb_streamd running."` |
| `kill_daemon` | Same code base as `edb_streamd` (identical string set, different binary): sends a kill request to the daemon's port. | cstrings `"%s:kill daemon"` |
| `rb_http_server` | Go 1.20 binary: local HTTP listener for OAuth 2.0 PKCE login redirects (`http://localhost:%d/redirect.html`), opens the browser. Not part of device export. | `main.openBrowser`, main binary strings `"./rb_http_server"`, `"code_challenge_method=S256"` |
| `rbinit` | Licence-manager initialiser (Sentinel/SafeNet style `/var/.slm*` paths). Not part of this subsystem; listed for completeness. | cstrings |
| `Upmgr rekordbox.app` | JUCE "Update Manager" app (`com.pioneer.Upmgr_rekordbox` 3.0.1.2): checks for update packages, downloads and installs with admin rights; localised messages. Not on the device path. | Info.plist, strings |
| `rekordboxAgent.app` | Background agent (cloud sync); referenced by restore messages. Out of scope. | `"The rekordboxAgent files cannot be found."` |

## 4. Internal architecture as observed

```
UI: WidgetProDJLink, WidgetLink*, RemoteSettingController, PlaylistSyncManager,
    BackupedDeviceLib*, DeviceLibraryPlusDlgManager, MySetting*Component, ExportModeLayoutSelector
        |
djplay::UiProDJLink (193 methods, listener hub, thread message queue, Wi-Fi sync queue)
        |
prodjlink::ProDJLink (singleton facade)  -- prodjlink::LinkDeviceManager, MobileDeviceManager
        |
prodjlink::LinkProxy  ->  InnerLinkAPI (LinkAPI Main Thread, timers, member list)
        |                          |
        |                  PSvLinkNetworkAccess (unicast/broadcast builders)
        |                  PSvLinkNormalInterval / ShortInterval / TimeBeatCounter
        |                  PSvLinkUDP (50000/50001/50002/50004), PSvLinkTCPServer/Client
        |                  PSvLinkFrameHeader + PSvLink*Info message codecs
        |                  PSvLinkMessageManager, psvlink::common::CommonMessageQueue
        |
PSvLinkProcess  -> executeSystemManager -> PSvLinkSysMgrMain / SysMgrMainComponent (claim, keep-alive, net monitor)
                -> executeNFSd          -> PSvNFSd / NFSdMainComponent (portmap, mountd, nfsd threads)
                (talk via PSvLinkIPC and /tmp/PSvLinkProc)
PSvDBServer -> PSvDBMain (remote DB command handler) -> PSvDBComm/PSvDBConnection (sockets)
            -> PSvMasterDBIF / PSvAppSyncDBIF (queries into the master library), PSvDBSharedDB
            -> edb_streamd (DeviceSQL daemon)
DB side: db::DatabaseMediator (export/import/backup orchestration)
         -> db::DevSqliteDBController (exportLibrary.db, SQLCipher)
         -> DeviceSQL DEV_MODULE* (export.pdb / exportExt.pdb), Dsql* helpers
         -> analyzer::CMusicAnalyzeFile (ANLZ writer)
         -> cdjDeviceSetting, pioneerMySetting, rekordboxExportSetting (settings files)
HID side: djplay::DeviceManager -> DeviceHid -> HidMap<Model> (+Sub) -> MAC_HidInput / HidRunLoopClass
          DeviceAuth/DeviceAuth2, ProductInfoRequest, DisplayDataManager + *Builder, PdjUsbHidOutputHelper
          HidMapLANHID for network HID
```

Notes:

- `PSvLinkProcess` names (`executeNFSd`, `executeSystemManager`, `terminateNFSd`) and the IPC class suggest the system manager and NFS server were once separate processes; in 7.2.19 there are no separate binaries for them, so they are threads inside `rekordbox` talking over a local socket **(inferred)**.
- `ProDJLinkCallback` (agent events, command log, function log, errors) and `PSvLinkLog` provide logging; `"LinkAPI Main Thread"`, `"PSvLinkUDP Thread"` etc. name the threads.
- `DatabaseMediator` is the single orchestrator for export, import, backup, restore and Sync Manager, and holds device handles (`getDeviceHandle`, `getAllDeviceHandle`, `get_device_controller`).
- `browse::CdjDispTree` is shared by HID control and SDP relays to render a CDJ-style browse tree.
- KUVO-related network analysis (`NetworkAnalyze`, `NetAnalysis`, `rb-share.kuvo.com/anlz/v1/...`) is cloud analysis sharing, not Pro DJ Link.

## 5. Prior art

| Project | Scope | Licence | Use for us |
|---|---|---|---|
| Deep Symmetry `dysentery` and the "DJ Link Ecosystem Analysis" site (djl-analysis.deepsymmetry.org) | Packet-level Pro DJ Link docs: ports, claim sequence, status, beat, sync, dbserver, NFS, touch audio | EPL-2.0 code; docs licence to verify | Primary protocol reference; read, do not copy code |
| Deep Symmetry `crate-digger` | Kaitai specs for PDB (`rekordbox_pdb.ksy`), ANLZ (`rekordbox_anlz.ksy`), settings files; Java NFS client | EPL-2.0 | Format reference; specs are facts we restate |
| Deep Symmetry `beat-link` | Java peer: virtual CDJ, metadata finder, sync, dbserver client | EPL-2.0 | Reference peer for interop tests |
| `rekordcrate` (Rust) | PDB, ANLZ and settings parsing with binrw; some write support | MPL-2.0 (verify) | Usable as a dependency (file-level copyleft); or reimplement |
| `pyrekordbox` | master.db (SQLCipher), PDB, ANLZ, XML, Device Library Plus | MIT | Reference for exportLibrary.db and key handling (do not copy key) |
| `prolink-connect` (TypeScript) | Pro DJ Link client: status, dbserver, NFS, metadata | MIT (verify) | Second reference peer |
| `python-prodj-link` | Python Pro DJ Link client and NFS | Apache-2.0 (verify) | Third reference |
| OneLibrary public material (AlphaTheta, press) | Confirms OneLibrary = Device Library Plus, shared with djay/Traktor/VirtualDJ | n/a | Product context only |

Sources: [Deep Symmetry DJ Link analysis](https://djl-analysis.deepsymmetry.org/), [crate-digger](https://github.com/Deep-Symmetry/crate-digger), [beat-link](https://github.com/Deep-Symmetry/beat-link), [dysentery](https://github.com/Deep-Symmetry/dysentery), [rekordcrate](https://github.com/Holzhaus/rekordcrate), [pyrekordbox](https://github.com/dylanljones/pyrekordbox), [prolink-connect](https://github.com/EvanPurkhiser/prolink-connect), [python-prodj-link](https://github.com/flesniak/python-prodj-link), [Digital DJ Tips on OneLibrary](https://www.digitaldjtips.com/onelibrary/), [AlphaTheta: VirtualDJ supports OneLibrary](https://alphatheta.com/en/information/virtualdj-officially-supports-onelibrary/), [NowPlaying OneLibrary notes](https://help.nowplayingapp.com/reference/integrations/onelibrary/).

Still unknown or thin in public work:

- DeviceSQL **write** rules: page allocation, row-group ordering, index pages, how rekordbox chooses string encodings and fills `reserved*` and unknown header fields. Public specs are read-oriented; players are tolerant but some fields matter (for example page counts, "next unused page").
- `exportExt.pdb` table layouts beyond what crate-digger has.
- `exportLibrary.db` exact DDL, `dbVersion` values, `analysedBits` bit meanings, `hasModified` and update-count semantics, and how players use the seek columns.
- `.2EX` vocal detection section layout and version.
- rekordbox's role as a **server** (dbserver and NFS): prior art is mostly client-side. Which requests players send to rekordbox (versus a USB stick in another player), and how rekordbox answers newer CDJ-3000 requests (extended cues, phrase data, multi waveform view).
- Link Export behaviour with CDJ-3000X, OPUS-QUAD and XDJ-AZ, including LAN-HID and port 50111.
- Mobile (RBM) Wi-Fi sync protocol: entirely undocumented publicly as far as known.
- HID authentication (deliberately not to be reproduced if it is a DRM/anti-clone check; see open question 6.12).

## 6. Open questions for Ghidra

1. `PSvLinkFrameHeader::writeData` / `readData` / `matchMessage`: exact header layout rekordbox writes (name field length, minor version byte, length field) and how `checkUpper` gates versions.
2. Each `PSvLink*Info::setData` / `matchMessage` (for example `PSvLinkDeviceConnect2SendInfo`, `PSvLinkSyncMasterReqSndInfo`, `PSvLinkBeatSndInfo`, `PSvLinkDevSettingWriteReqInfo`, `PSvLinkDJMMySetting::WriteReq`): type byte, port and payload layout of every packet rekordbox emits.
3. `SysMgrMainComponent::frameSendDiscoveryRequest`, `frameSendIdSetRequest`, `frameSendIdUseRequest`, `readCompatiRes`, `suspendMode23`: claim sequence timing, which player numbers rekordbox prefers, and what "Mode23" is.
4. `InnerLinkAPI::expTimer`, `expbeatInfoTimer`, `expmsTimerSyncMaster`, `PSvLinkNormalInterval::countTimer`: keep-alive, status and beat intervals.
5. `PSvLinkUDP::startRecv` / `PSvLinkTCPServer::run`: which ports are bound by whom, and the purpose of UDP 50111.
6. `PSvDBMain::OnClientReq` and `OnListClientCmd`, `OnSongAnlzCmd`, `OnMAnlzClientCmd`, `RetNewCueToClient`: dbserver request type table and response layouts as served by rekordbox; port returned for the `"RemoteDBServer"` query.
7. `PSvDBMain::executeStreamD` and `edb_streamd` main: arguments, environment (`EDB_LISTEN_PORT`, `EDB_FIFO_PATH`, `EDB_BIN`, `E3E_PATH`) and whether we need any equivalent (likely not).
8. `NFSdMainComponent::startNFSd`, `addNFSPermission`, `setPermittedFileExtensionList`: NFS version, export paths presented to players, file handle scheme, permission model.
9. `db::DatabaseMediator::export_track_data_to_db`, `export_list`, `export_list_track`, `exportFile`: order of writes, which DB(s) are written for each `kDeviceDBType` value, and failure rollback.
10. `edb_DEV_MODULE*_table_inits` and `DevAddContent`, `DevAddPlaylist`, `DevSetAnalysisData`: DeviceSQL table IDs, row layouts and the generated insert logic for `export.pdb`.
11. `db::DevSqliteDBController::createTables` / `createIndexes` / `initPropertyTable`: full DDL and initial rows for `exportLibrary.db`. (Key derivation in `DevSQLiteDB::openDB` should be identified only to the extent of "which public key handling applies"; do not record the key.)
12. `djplay::DeviceAuth::makeHash` / `makePassword` / `getTable`: determine whether HID auth is an anti-clone/licence check. If so, Cuepoint does not reimplement it and HID control of such models is out of scope; if it is a plain handshake documented elsewhere, record the protocol (not the table).
13. `analyzer::CMusicAnalyzeFile::write_pmai_ex2_vocal_detect`, `write_pmai_ext_song_struct`, `write_pmai_ext_cue_ext_data`: tag codes, layouts, and whether phrase data is masked.
14. `analyzer::CMusicAnalyzeFile::make_anlz_file_path`: the `USBANLZ` path hash for exports and link serving.
15. `cdjDeviceSetting::calcCRC`, `pioneerMySetting::calcCRC`, `pioneerMySetting::MySettingData::setDataChunk`, `DJMMSContent::writeData`: CRC polynomial, header strings, field offsets and enumerations.
16. `RekordboxExportSettingFile::createSettingFile`, `checkVersion`: name and format of the export setting file (background colour, content filter, log-out time, hot cue colour).
17. `DeviceSyncSetting::write`, `MasterSyncSetting`: XML schema of `sync*.temp` and `masterPlaylists6.xml`.
18. `DeviceHistory::ImportedHistoryManager::getHash` and `serializeToXml`: dedupe hash and XML schema of `ImportedHistory.xml`.
19. `db::DatabaseMediator::backupDeviceLibrary`, `db::BackupZipBuilder::addFile`: backup archive layout and naming (`rbDevLibBa_`, `rbDevLibBaInfo_`, `backup_generation`).
20. `djplay::ControllerDisplay2Builder::setFieldMapForCDJ3000` and `HidMapCDJ3000::sendWholeWaveformData`: HID display report layouts for CDJ-3000.
21. `prodjlink::ProDJLink::dragDropPlayMusic` path to `PSvLinkNetworkAccess::instPlayMusic`: load-track command fields (slot, track type, rekordbox ID).
22. `ProDJLink::enableLinkMonitor` / `PSvLinkMonitorDataInfo`: what Link Monitor transports (audio? status?).

## 7. Rust plan

### 7.1 Crates and modules

`cuepoint-export` (files on media; no networking):

- `layout`: device root detection (`PIONEER` vs `.PIONEER`), path builders for `Contents`, `USBANLZ`, `Artwork`, settings files; file name sanitising and 255-character limit.
- `pdb`: DeviceSQL page reader and writer for `export.pdb` and `exportExt.pdb` (tables, row groups, strings, indexes). Writer first targets "players accept it", then "byte-identical where feasible".
- `devlib_plus`: `exportLibrary.db` schema, writer and reader via SQLCipher.
- `anlz`: `.DAT`/`.EXT`/`.2EX` reader and writer, sharing types with `cuepoint-analysis` (beat grid, cues, waveforms, phrases).
- `artwork`: resize and JPEG encode (320x320 and small variants).
- `settings`: `MYSETTING*.DAT`, `DJMMYSETTING.DAT`, `DEVSETTING.DAT`, export setting and content filter files, with CRC.
- `sync`: Sync Manager state files, ImportedHistory XML, device-library backup and restore archives.
- `job`: export/import orchestration with progress, cancellation, format restriction checks and the export log; drives `cuepoint-library` for source data.
- `removable` (or in `cuepoint-core`): drive enumeration, file system type, free space, eject (macOS `DiskArbitration` via `objc2` crates, Linux `udisks2` over D-Bus via `zbus`, Windows via `windows` crate).

`cuepoint-prolink` (network peer):

- `packet`: typed codecs for 50000/50001/50002/50004 packets (header, device name, per-type bodies), zero-copy parse plus builders.
- `peer`: device number claim, keep-alive, member table, link up/down monitoring per interface.
- `status`: player/mixer status tracking, on-air, BPM, beat position, master handoff, sync control.
- `dbserver`: remote DB server (port query service on 12523 and the message protocol), backed by a `LibrarySource` trait implemented by `cuepoint-library` (collection) and by `cuepoint-export` (a mounted device, for testing).
- `nfs`: user-space portmapper, MOUNT and NFS v2 (UDP) server exposing a read-only virtual tree of collection files.
- `link_export`: glue: start/stop, port checks, renew broadcasts, load-to-player, write-back of cues/history/BPM into the library.
- `mobile`: placeholder until the RBM protocol is understood.

`cuepoint-control` (HID): `hid::cdj` maps per model, display feedback builders; uses `hidapi` (C, see below) or a pure-Rust IOKit/hidraw layer. Gated by the outcome of open question 12.

### 7.2 Dependencies (pure Rust unless flagged)

- Binary parsing/writing: `binrw` (MIT) or hand-written `byteorder`/`zerocopy` codecs. Optionally `rekordcrate` (MPL-2.0) for reading; decide after checking its writer coverage and licence fit with `MIT OR Apache-2.0` (MPL is compatible as a dependency, but its files stay MPL).
- Networking: `tokio` + `socket2` (broadcast, `SO_REUSEADDR`), `if-addrs` or `netdev` for interfaces, `crc` for CRCs.
- NFS/RPC: no mature pure-Rust NFS v2 server; write our own XDR (`xdr-codec` or hand-rolled). `nfsserve` (pure Rust, NFS v3, BSD-3) could be a starting point if players accept v3 (they likely do not; prior art says v2).
- SQLCipher: **C dependency**. `rusqlite` with `bundled-sqlcipher` builds SQLCipher C sources and needs a crypto backend (OpenSSL or, on macOS, CommonCrypto). No pure-Rust SQLCipher exists; options: accept the C dependency behind a feature flag, or implement SQLCipher's page encryption (AES-256-CBC + HMAC per page, PBKDF2 key derivation) on top of a pure-Rust SQLite reader/writer, which is a large project. Recommendation: accept `rusqlite`/SQLCipher (C) for v1, isolate behind `devlib_plus`.
- Images: `image` (pure Rust JPEG encode/decode via `zune-jpeg`/`jpeg-encoder`).
- Audio seek info for cue seek columns: from `cuepoint-engine` decoders (`symphonia`, pure Rust).
- XML: `quick-xml`. Zip for backups: `zip` (pure Rust with `deflate` via `miniz_oxide`).
- HID: `hidapi` is **C** (wraps platform APIs); pure-Rust alternatives are platform specific (`io-kit-sys` FFI on macOS is still FFI). Flag for review.
- Removable media on macOS: `objc2-disk-arbitration` (FFI to system framework, acceptable as OS API, not a bundled C library).

### 7.3 Parity test strategy

1. **Golden exports.** Build a fixture library (licence-free audio: generated tones and CC0 tracks), export it with real rekordbox 7.2.19 to a disk image for each `kDeviceDBType` setting, and store the outputs outside the repo (they contain AlphaTheta-generated data). Tests export the same library with Cuepoint and compare:
   - ANLZ: section-by-section structural equality, byte equality for sections whose inputs we control (path, cues, beat grid given identical grid input); waveform sections compared with tolerances, since analysis algorithms differ.
   - `export.pdb`: logical equality (all rows in all tables) via our reader and crate-digger's reader; byte equality as a stretch goal.
   - `exportLibrary.db`: decrypt both, compare schema DDL and row contents.
   - Settings files: byte equality for identical settings.
2. **Cross-reader checks.** Every file Cuepoint writes must parse cleanly with crate-digger (Kaitai), rekordcrate and pyrekordbox in CI (run as external tools, not linked).
3. **Round trip.** Export from Cuepoint, import into real rekordbox 7.2.19 ("Import playlist from device", device browse) and check no errors and identical metadata; and the reverse (rekordbox export read by Cuepoint import).
4. **Hardware.** Manual test matrix on real players: CDJ-2000NXS2 (legacy PDB only), CDJ-3000/3000X and XDJ-AZ or OPUS-QUAD (OneLibrary), XDJ-RX3. Checks: library browse, all categories, playback, hot cues and colours, waveforms (blue, RGB, 3-band), phrases, My Settings applied, history recorded and re-imported.
5. **Pro DJ Link interop.** In CI, run beat-link (and prolink-connect) as virtual peers on a loopback or veth network: Cuepoint must claim a number, appear in their device lists, answer dbserver metadata, artwork, waveform, beat grid and cue requests, and serve files over NFS (use crate-digger's NFS client or beat-link's metadata finder). Packet captures from real rekordbox Link Export sessions (pcap, kept outside the repo) are replayed against Cuepoint's codecs: every captured packet must parse, and Cuepoint-built equivalents must match field by field. A Wireshark dissector from prior art can be used to diff captures.
6. **Hardware link sessions.** Real CDJs on a switch with rekordbox replaced by Cuepoint: browse, load, play, set a cue on the CDJ and confirm it lands in Cuepoint's library; sync master handoff both ways; load-to-player from Cuepoint.
7. **Fuzzing.** `cargo-fuzz` targets for every parser (PDB pages, ANLZ sections, settings files, Pro DJ Link packets, dbserver messages, XDR), since devices on the network and media files are untrusted input.
