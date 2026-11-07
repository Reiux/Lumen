// Tauri-backed implementation of ConfigManager maintaining exact signatures and return types

import { hasTauri, tauriInvoke } from './wails-runtime';
import * as $models from '../../bindings/app/core/models';

export const ConfigManager = {
  async AddCredentials(newAuthString: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('add_credentials', { newAuthString });
    }
  },

  async AddGoogleAccount(oauthToken: string): Promise<string> {
    if (hasTauri()) {
      return tauriInvoke('add_google_account', { oauthToken });
    }
    return 'mock@example.com';
  },

  async AddRecentAlbum(name: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('add_recent_album', { name });
    }
  },

  async AddTokenBindingAliasFromADB(email: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('add_token_binding_alias_from_adb', { email });
    }
  },

  async AddWatchedFolder(folder: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('add_watched_folder', { folder });
    }
  },

  async AddWatchedFolderWithAccount(folder: string, accountEmail: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('add_watched_folder_with_account', { folder, accountEmail });
    }
  },

  async CancelBrowserSignIn(): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('cancel_browser_sign_in');
    }
  },

  async CancelScan(): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('cancel_scan');
    }
  },

  async CheckForUpdates(manual: boolean): Promise<$models.UpdateInfo> {
    if (hasTauri()) {
      const res = await tauriInvoke('check_for_updates', { manual });
      return $models.UpdateInfo.createFrom(res);
    }
    return new $models.UpdateInfo({
      currentVersion: '1.1.0',
      latestVersion: '1.1.0',
      hasUpdate: false,
      releaseUrl: 'https://github.com/Reiux/Lumen/releases/latest',
      releaseNotes: '',
      publishedAt: new Date().toISOString(),
      checkedAt: new Date().toISOString(),
    });
  },

  async CheckForUpdatesWithRedirectURL(redirectURL: string, manual: boolean): Promise<$models.UpdateInfo> {
    if (hasTauri()) {
      const res = await tauriInvoke('check_for_updates_with_redirect_url', { redirectURL, manual });
      return $models.UpdateInfo.createFrom(res);
    }
    return this.CheckForUpdates(manual);
  },

  async ClearHashCache(): Promise<number> {
    if (hasTauri()) {
      return (await tauriInvoke<number>('clear_hash_cache')) || 0;
    }
    return 0;
  },

  async ClearLogEntries(): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('clear_log_entries');
    }
  },

  async CountFolderBacklog(folder: string): Promise<number> {
    if (hasTauri()) {
      return (await tauriInvoke<number>('count_folder_backlog', { folder })) || 0;
    }
    return 0;
  },

  async CredentialNeedsTokenBinding(authString: string): Promise<boolean> {
    if (hasTauri()) {
      return (await tauriInvoke<boolean>('credential_needs_token_binding', { authString })) || false;
    }
    return false;
  },

  async ExportLogs(format: string): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('export_logs', { format })) || '';
    }
    return '';
  },

  async GetAccountForPath(filePath: string): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('get_account_for_path', { filePath })) || '';
    }
    return '';
  },

  async GetAccounts(): Promise<$models.AccountsState> {
    if (hasTauri()) {
      const res = await tauriInvoke('get_accounts');
      return $models.AccountsState.createFrom(res);
    }
    return new $models.AccountsState({ accounts: [], selected: '' });
  },

  async GetAlbumAutoMode(): Promise<boolean> {
    if (hasTauri()) {
      return (await tauriInvoke<boolean>('get_album_auto_mode')) || false;
    }
    return false;
  },

  async GetAlbumName(): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('get_album_name')) || '';
    }
    return '';
  },

  async GetAlbumNickname(account: string, key: string): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('get_album_nickname', { account, key })) || '';
    }
    return '';
  },

  async GetAlbumNicknames(account: string): Promise<{ [_ in string]?: string }> {
    if (hasTauri()) {
      return (await tauriInvoke<{ [_ in string]?: string }>('get_album_nicknames', { account })) || {};
    }
    return {};
  },

  async GetAppVersion(): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('get_app_version')) || '1.1.0';
    }
    return '1.1.0';
  },

  async GetAutoCheckUpdates(): Promise<boolean> {
    if (hasTauri()) {
      return (await tauriInvoke<boolean>('get_auto_check_updates')) || false;
    }
    return false;
  },

  async GetExcludePattern(): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('get_exclude_pattern')) || '';
    }
    return '';
  },

  async GetHashCacheCount(): Promise<number> {
    if (hasTauri()) {
      return (await tauriInvoke<number>('get_hash_cache_count')) || 0;
    }
    return 0;
  },

  async GetInotifyError(): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('get_inotify_error')) || '';
    }
    return '';
  },

  async GetLastUpdateCheck(): Promise<number> {
    if (hasTauri()) {
      return (await tauriInvoke<number>('get_last_update_check')) || 0;
    }
    return 0;
  },

  async GetLogEntries(limit: number, levelFilter: string): Promise<$models.LogEntry[]> {
    if (hasTauri()) {
      const res = await tauriInvoke<any[]>('get_log_entries', { limit, levelFilter });
      return (res || []).map((e) => $models.LogEntry.createFrom(e));
    }
    return [];
  },

  async GetProblemLogs(): Promise<$models.LogEntry[]> {
    if (hasTauri()) {
      const res = await tauriInvoke<any[]>('get_problem_logs');
      return (res || []).map((e) => $models.LogEntry.createFrom(e));
    }
    return [];
  },

  async GetRecentAlbums(): Promise<string[]> {
    if (hasTauri()) {
      return (await tauriInvoke<string[]>('get_recent_albums')) || [];
    }
    return [];
  },

  async GetReleaseNotes(tag: string): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('get_release_notes', { tag })) || '';
    }
    return '';
  },

  async GetSettings(): Promise<$models.Preferences> {
    if (hasTauri()) {
      const res = await tauriInvoke('get_settings');
      return $models.Preferences.createFrom(res);
    }
    return new $models.Preferences();
  },

  async GetSupportedBrowserName(): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('get_supported_browser_name')) || 'Google Chrome';
    }
    return 'Google Chrome';
  },

  async GetWatchedFolderConfigForPath(filePath: string): Promise<$models.WatchedFolderConfig | null> {
    if (hasTauri()) {
      const res = await tauriInvoke('get_watched_folder_config_for_path', { filePath });
      return res ? $models.WatchedFolderConfig.createFrom(res) : null;
    }
    return null;
  },

  async GetWatchedFolderConfigs(): Promise<$models.WatchedFolderConfig[]> {
    if (hasTauri()) {
      const res = await tauriInvoke<any[]>('get_watched_folder_configs');
      return (res || []).map((c) => $models.WatchedFolderConfig.createFrom(c));
    }
    return [];
  },

  async GetWatchedFolders(): Promise<string[]> {
    if (hasTauri()) {
      return (await tauriInvoke<string[]>('get_watched_folders')) || [];
    }
    return [];
  },

  async HasSupportedBrowser(): Promise<boolean> {
    if (hasTauri()) {
      return (await tauriInvoke<boolean>('has_supported_browser')) || false;
    }
    return true;
  },

  async IsAccountRiskAcknowledged(): Promise<boolean> {
    if (hasTauri()) {
      return (await tauriInvoke<boolean>('is_account_risk_acknowledged')) || false;
    }
    return true;
  },

  async IsDebugBuild(): Promise<boolean> {
    if (hasTauri()) {
      return (await tauriInvoke<boolean>('is_debug_build')) || false;
    }
    return false;
  },

  async IsTrayAvailable(): Promise<boolean> {
    if (hasTauri()) {
      return (await tauriInvoke<boolean>('is_tray_available')) || false;
    }
    return true;
  },

  async PickUploadFiles(): Promise<string[]> {
    if (hasTauri()) {
      return (await tauriInvoke<string[]>('pick_upload_files')) || [];
    }
    return [];
  },

  async PickUploadFolder(): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('pick_upload_folder')) || '';
    }
    return '';
  },

  async PickWatchedFolder(): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('pick_watched_folder')) || '';
    }
    return '';
  },

  async RemoveCredentials(email: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('remove_credentials', { email });
    }
  },

  async RemoveRecentAlbum(name: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('remove_recent_album', { name });
    }
  },

  async RemoveWatchedFolder(folder: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('remove_watched_folder', { folder });
    }
  },

  async SetAccountRiskAcknowledged(acknowledged: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_account_risk_acknowledged', { acknowledged });
    }
  },

  async SetAlbumAutoMode(autoMode: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_album_auto_mode', { autoMode });
    }
  },

  async SetAlbumName(albumName: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_album_name', { albumName });
    }
  },

  async SetAlbumNickname(account: string, key: string, nickname: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_album_nickname', { account, key, nickname });
    }
  },

  async SetAutoCheckUpdates(enabled: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_auto_check_updates', { enabled });
    }
  },

  async SetDeleteFromHost(deleteFromHost: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_delete_from_host', { deleteFromHost });
    }
  },

  async SetDisableUnsupportedFilesFilter(disableUnsupportedFilesFilter: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_disable_unsupported_files_filter', { disableUnsupportedFilesFilter });
    }
  },

  async SetExcludePattern(pattern: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_exclude_pattern', { pattern });
    }
  },

  async SetForceUpload(forceUpload: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_force_upload', { forceUpload });
    }
  },

  async SetLastUpdateCheck(timestamp: number): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_last_update_check', { timestamp });
    }
  },

  async SetMinimizeToTray(v: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_minimize_to_tray', { v });
    }
  },

  async GetLaunchOnStartup(): Promise<boolean> {
    if (hasTauri()) {
      return tauriInvoke('get_launch_on_startup');
    }
    return false;
  },

  async SetLaunchOnStartup(v: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_launch_on_startup', { v });
    }
  },

  async SetNotifyOnComplete(v: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_notify_on_complete', { v });
    }
  },

  async SetPairLivePhotos(pairLivePhotos: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_pair_live_photos', { pairLivePhotos });
    }
  },

  async SetProxy(proxy: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_proxy', { proxy });
    }
  },

  async SetRecursive(recursive: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_recursive', { recursive });
    }
  },

  async SetSaver(saver: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_saver', { saver });
    }
  },

  async SetSelected(email: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_selected', { email });
    }
  },

  async SetSetDateFromFilename(v: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_set_date_from_filename', { v });
    }
  },

  async SetSkipIncompleteLivePhotos(skipIncompleteLivePhotos: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_skip_incomplete_live_photos', { skipIncompleteLivePhotos });
    }
  },

  async SetUpdateExistingPhotosToLive(updateExistingPhotosToLive: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_update_existing_photos_to_live', { updateExistingPhotosToLive });
    }
  },

  async SetUploadSpeedLimitKBps(v: number): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_upload_speed_limit_kbps', { v });
    }
  },

  async SetUploadThreads(uploadThreads: number): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_upload_threads', { uploadThreads });
    }
  },

  async SetUseLocalHashCache(v: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_use_local_hash_cache', { v });
    }
  },

  async SetUseQuota(useQuota: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_use_quota', { useQuota });
    }
  },

  async SetWatchFoldersEnabled(v: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_watch_folders_enabled', { v });
    }
  },

  async StartBrowserSignIn(): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('start_browser_sign_in')) || '';
    }
    return 'mock@example.com';
  },

  async StartBrowserSignInWithOptions(opts: $models.BrowserSignInOptions): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('start_browser_sign_in_with_options', { opts })) || '';
    }
    return 'mock@example.com';
  },

  async UpdateWatchedFolderConfig(cfg: $models.WatchedFolderConfig): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('update_watched_folder_config', { cfg });
    }
  },

  async ValidateAndAddCredentials(rawInput: string): Promise<string> {
    if (hasTauri()) {
      return (await tauriInvoke<string>('validate_and_add_credentials', { rawInput })) || '';
    }
    return 'mock@example.com';
  },

  async WasBrowserCloseFailed(): Promise<boolean> {
    if (hasTauri()) {
      return (await tauriInvoke<boolean>('was_browser_close_failed')) || false;
    }
    return false;
  },
};
