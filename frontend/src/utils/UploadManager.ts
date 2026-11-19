import { Clipboard, Events } from "@wailsio/runtime";
import { reactive } from "vue";
import {
  recordUploadResult,
  type UploadResults,
} from "./uploadResults";

export interface ThreadStatus {
  WorkerID: number;
  Status: string;
  FilePath: string;
  FileName: string;
  Message: string;
  BytesUploaded: number;
  BytesTotal: number;
  Attempt: number;
}

export interface FileUploadResult {
  MediaKey: string;
  IsError: boolean;
  IsLivePhoto: boolean;
  Skipped: boolean;
  ErrorMessage: string;
  SkipCode: string;
  SkipReason: string;
  Path: string;
  Paths: string[];
  SizeBytes?: number;
}

export interface PreflightWarning {
  Paths: string[];
  Code: string;
  Message: string;
}

export interface UploadBatchStart {
  Total: number;
  TotalBytes: number;
}

export interface AlbumStatus {
  AlbumName: string;
  ItemsAdded: number;
  TotalItems: number;
  AlbumKeys: string[];
  IsComplete: boolean;
}

export interface AlbumError {
  AlbumName: string;
  Error: string;
}

function isSkippedUploadWarning(code: string): boolean {
  return code === "incomplete-live-photo-skipped" || code === "ambiguous-filename-stem";
}

export interface AccountBlockedState {
  isBlocked: boolean;
  reason: 'SIGN_IN_REQUIRED' | 'ACCOUNT_RESTRICTED' | 'RATE_LIMITED' | '';
  email: string;
  message: string;
  resumeAt?: number;
}

export interface ScanProgressState {
  isScanning: boolean;
  currentDirectory: string;
  filesFound: number;
  /**
   * UI phase while the screen is visible:
   * - "scanning": backend is traversing folders (currentDirectory updates live)
   * - "preparing": traversal finished; batch is being classified and hashed
   *   before the first worker reports activity.
   */
  phase?: 'scanning' | 'preparing';
}

export interface UploadState {
  isUploading: boolean;
  isPaused: boolean;
  isDryRun: boolean;
  totalFiles: number;
  uploadedFiles: number;
  threads: Map<number, ThreadStatus>;
  results: UploadResults;
  warnings: PreflightWarning[];
  // Byte tracking
  totalBytes: number;
  uploadedBytes: number;
  // Timing
  startTime: number;
  // Speed calculation (bytes per second)
  uploadSpeed: number;
  // Album creation
  albumStatus: AlbumStatus | null;
  isCreatingAlbum: boolean;
  // Circuit breaker state
  accountBlocked: AccountBlockedState;
  // Incremental folder scanning state
  scanProgress: ScanProgressState;
}

class UploadManager {
  private static instance: UploadManager;

  // Reactive state that can be accessed by components
  public state = reactive<UploadState>({
    isUploading: false,
    isPaused: false,
    isDryRun: false,
    totalFiles: 0,
    uploadedFiles: 0,
    threads: new Map<number, ThreadStatus>(),
    results: {
      success: [],
      fail: [],
      skipped: [],
      warnings: [],
    },
    warnings: [],
    totalBytes: 0,
    uploadedBytes: 0,
    startTime: 0,
    uploadSpeed: 0,
    albumStatus: null,
    isCreatingAlbum: false,
    accountBlocked: {
      isBlocked: false,
      reason: '',
      email: '',
      message: '',
    },
    scanProgress: {
      isScanning: false,
      currentDirectory: '',
      filesFound: 0,
    },
  });

  // For speed calculation
  private lastSpeedUpdate: number = 0;
  private lastBytesUploaded: number = 0;
  private speedSamples: number[] = [];
  // Track bytes from completed files
  private completedBytes: number = 0;
  // Track the last known BytesTotal for each file
  private fileBytes: Map<string, number> = new Map();
  // Dedupe decisions can change the bytes that will actually be uploaded.
  private totalBytesAdjustment: number = 0;
  // True between the moment the user picks/drops files and the first upload
  // worker reporting activity. Keeps the indexing/preparation screen sticky.
  private preparationActive: boolean = false;

  private constructor() {
    // Bind all methods to ensure 'this' context is preserved
    this.resetUploadResults = this.resetUploadResults.bind(this);
    this.cancelUpload = this.cancelUpload.bind(this);
    this.copyResultsAsJson = this.copyResultsAsJson.bind(this);

    this.setupEventListeners();
  }

  public static getInstance(): UploadManager {
    if (!UploadManager.instance) {
      UploadManager.instance = new UploadManager();
    }
    return UploadManager.instance;
  }

  private setupEventListeners() {
    // Handle upload start
    Events.On("uploadStart", (event: { data: UploadBatchStart }) => {
      const data = (event?.data || {}) as Record<string, any>;
      const totalFiles = Number(data.Total ?? data.total ?? 0);
      const totalBytes = Number(data.TotalBytes ?? data.totalBytes ?? 0);
      const isDryRun = Boolean(data.IsDryRun ?? data.isDryRun ?? false);

      this.resetUploadResults();
      this.state.totalFiles = totalFiles;
      this.state.totalBytes = totalBytes; // May be 0 initially
      this.state.isDryRun = isDryRun;
      this.state.isPaused = false;
      this.state.albumStatus = null;
      this.state.isCreatingAlbum = false;
      this.state.threads.clear();
      this.state.startTime = Date.now();
      this.state.uploadSpeed = 0;
      this.lastSpeedUpdate = Date.now();
      this.lastBytesUploaded = 0;
      this.speedSamples = [];
      this.completedBytes = 0;
      this.fileBytes.clear();
      this.totalBytesAdjustment = 0;
      this.state.warnings = [];

      if (totalFiles === 0) {
        this.state.isUploading = false;
        this.preparationActive = false;
        this.state.scanProgress.isScanning = false;
        window.dispatchEvent(new CustomEvent('uploadEmpty'));
        return;
      }

      this.state.isUploading = true;
    });

    // Handle async total bytes update (calculated after uploadStart)
    Events.On("uploadTotalBytes", (event: { data: number }) => {
      this.state.totalBytes = Math.max(0, event.data + this.totalBytesAdjustment);
    });

    Events.On("uploadTotalBytesDelta", (event: { data: number }) => {
      this.totalBytesAdjustment += event.data;
      this.state.totalBytes = Math.max(0, this.state.totalBytes + event.data);
    });

    Events.On("uploadWarning", (event: { data: PreflightWarning }) => {
      this.state.warnings.push(event.data);
      if (!isSkippedUploadWarning(event.data.Code)) {
        this.state.results.warnings.push({
          paths: event.data.Paths,
          code: event.data.Code,
          reason: event.data.Message,
        });
      }
    });

    // Handle thread status updates
    Events.On("ThreadStatus", (event: { data: ThreadStatus }) => {
      const thread = event.data;
      const prevThread = this.state.threads.get(thread.WorkerID);

      // First real worker activity (hashing/checking/uploading/...) means the
      // batch is prepared — hand over from the preparation screen to the
      // upload view.
      if (thread.Status !== 'idle') {
        this.finishPreparationIfActive();
      }

      if (thread.Status === 'uploading' && thread.FilePath && thread.BytesTotal > 0) {
        this.fileBytes.set(thread.FilePath, thread.BytesTotal);
      }

      if (thread.Status === 'error' && prevThread?.Message !== thread.Message) {
        window.dispatchEvent(new CustomEvent('uploadError', { detail: thread }));
      }
      
      this.state.threads.set(thread.WorkerID, thread);
      this.updateBytesAndSpeed();
    });

    // Handle file status updates
    Events.On("FileStatus", (event: { data: FileUploadResult }) => {
      const { Path, Paths } = event.data;

      // A file result means the batch is actively processing — hand over from
      // the preparation screen to the upload view.
      this.finishPreparationIfActive();

      this.state.uploadedFiles += recordUploadResult(this.state.results, event.data);
      if (!event.data.IsError) {
        const completedFileBytes = this.fileBytes.get(Path) || Number(event.data.SizeBytes || 0);
        if (completedFileBytes > 0) {
          this.completedBytes += completedFileBytes;
        }
      }
      for (const completedPath of Paths?.length ? Paths : [Path]) {
        this.fileBytes.delete(completedPath);
      }
      this.updateBytesAndSpeed();

      // Skipped-only batches can begin and end within one backend event burst.
      // Finishing locally prevents a missed terminal event from leaving the timer running.
      if (
        this.state.totalFiles > 0
        && this.state.uploadedFiles >= this.state.totalFiles
        && this.state.results.success.length === 0
        && this.state.results.fail.length === 0
      ) {
        this.state.isUploading = false;
      }
    });

    // Handle upload stop
    Events.On("uploadStop", () => {
      this.state.isUploading = false;
      this.preparationActive = false;
      this.state.scanProgress.isScanning = false;
    });

    Events.On("upload:error", (event: { data?: string } | string) => {
      const msg = typeof event === 'string' ? event : String(event?.data || 'Failed to start upload batch');
      this.endPreparation();
      window.dispatchEvent(new CustomEvent('uploadError', { detail: { Message: msg } }));
    });

    // Handle incremental folder scan progress
    Events.On("scan:progress", (event: { data: { currentDirectory: string; filesFound: number; isScanning: boolean } }) => {
      const data = event.data || ({} as { currentDirectory: string; filesFound: number; isScanning: boolean });
      // When the backend finishes traversing while the batch is still being
      // classified/hashed, keep the preparation screen up until the first
      // worker reports activity. Backend-initiated scans (watched folders)
      // without a frontend preparation phase clear immediately as before.
      if (data.isScanning === false && this.preparationActive) {
        this.state.scanProgress = {
          isScanning: true,
          currentDirectory: '',
          filesFound: data.filesFound || this.state.scanProgress.filesFound,
          phase: 'preparing',
        };
        return;
      }
      this.state.scanProgress = {
        isScanning: data.isScanning ?? true,
        currentDirectory: data.currentDirectory || '',
        filesFound: data.filesFound || 0,
        phase: data.isScanning === false ? undefined : 'scanning',
      };
    });

    // Handle folder scan cancelled
    Events.On("scan:cancelled", () => {
      this.preparationActive = false;
      this.state.scanProgress = {
        isScanning: false,
        currentDirectory: '',
        filesFound: 0,
      };
      this.state.isUploading = false;
    });

    // Handle album creation progress
    Events.On("albumProgress", (event: { data: AlbumStatus }) => {
      this.state.albumStatus = event.data;
      this.state.isCreatingAlbum = true;
    });

    // Handle album creation complete
    Events.On("albumComplete", (event: { data: AlbumStatus }) => {
      this.state.albumStatus = event.data;
      this.state.isCreatingAlbum = false;
    });

    // Handle album creation error
    Events.On("albumError", (event: { data: AlbumError }) => {
      this.state.isCreatingAlbum = false;
      // Emit a custom event that App.vue can listen to for showing toast
      window.dispatchEvent(new CustomEvent('albumError', { detail: event.data }));
    });

    // Handle circuit breaker account blocked / rate limited events
    Events.On("account:blocked", (event: { data: { reason: string; email: string; message: string; resumeAt?: number } }) => {
      this.state.isUploading = false;
      this.state.accountBlocked = {
        isBlocked: true,
        reason: (event.data?.reason || 'SIGN_IN_REQUIRED') as AccountBlockedState['reason'],
        email: event.data?.email || '',
        message: event.data?.message || '',
        resumeAt: event.data?.resumeAt,
      };
      window.dispatchEvent(new CustomEvent('accountBlocked', { detail: event.data }));
    });
  }

  public clearAccountBlocked() {
    this.state.accountBlocked = {
      isBlocked: false,
      reason: '',
      email: '',
      message: '',
      resumeAt: undefined,
    };
  }

  public isUploadBlocked(): boolean {
    const blocked = this.state.accountBlocked;
    if (!blocked.isBlocked) return false;
    if (blocked.reason === 'RATE_LIMITED' && blocked.resumeAt && Date.now() >= blocked.resumeAt) {
      this.clearAccountBlocked();
      return false;
    }
    return true;
  }

  /**
   * Show the indexing/preparation screen immediately when the user picks or
   * drops files, before the backend starts scanning. The screen stays up
   * (sticky) until the first upload worker reports activity, so the UI never
   * shows an empty "0 out of 0" upload view while files are being classified
   * and hashed.
   */
  public beginPreparation() {
    // When a batch is already uploading, the new selection is queued by the
    // backend and starts after the current one — keep the upload view visible
    // instead of covering it with the preparation screen.
    if (this.state.isUploading) return;
    this.preparationActive = true;
    this.state.scanProgress = {
      isScanning: true,
      currentDirectory: '',
      filesFound: 0,
      phase: 'preparing',
    };
  }

  /** Hide the preparation screen (e.g. when the upload failed to start). */
  public endPreparation() {
    this.preparationActive = false;
    this.state.scanProgress = {
      isScanning: false,
      currentDirectory: '',
      filesFound: 0,
    };
  }

  private finishPreparationIfActive() {
    if (this.preparationActive) {
      this.preparationActive = false;
      this.state.scanProgress.isScanning = false;
      this.state.scanProgress.currentDirectory = '';
    }
  }

  public async cancelScan() {
    this.state.scanProgress.isScanning = false;
    this.preparationActive = false;
    try {
      const { ConfigManager } = await import('../../bindings/app/core');
      await ConfigManager.CancelScan();
    } catch (e) {
      console.warn("Failed to cancel scan:", e);
    }
    // If the scan already finished (batch is being classified/hashed), the
    // scan-cancel flag is no longer polled anywhere — cancel the batch too so
    // "Cancel Indexing" keeps working through the whole preparation window.
    try {
      await Events.Emit("uploadCancel");
    } catch {
      // Ignore — nothing to cancel
    }
  }

  public async cancelReviewScan() {
    this.state.scanProgress.isScanning = false;
    try {
      const { ConfigManager } = await import('../../bindings/app/core');
      await ConfigManager.CancelScan();
    } catch (e) {
      console.warn("Failed to cancel review scan:", e);
    }
  }

  private updateBytesAndSpeed() {
    // Calculate bytes from currently active uploads
    let activeUploadedBytes = 0;

    this.state.threads.forEach((thread) => {
      if (thread.Status === 'uploading' && thread.BytesTotal > 0) {
        activeUploadedBytes += thread.BytesUploaded;
      } else if (thread.Status === 'finalizing' && thread.FilePath) {
        activeUploadedBytes += this.fileBytes.get(thread.FilePath) ?? 0;
      }
    });

    // Total uploaded = completed files + current progress
    const totalUploaded = this.completedBytes + activeUploadedBytes;
    this.state.uploadedBytes = totalUploaded;

    // Calculate speed (using rolling average)
    const now = Date.now();
    const timeDelta = now - this.lastSpeedUpdate;

    if (timeDelta >= 500) { // Update speed every 500ms
      const bytesDelta = totalUploaded - this.lastBytesUploaded;
      const instantSpeed = (bytesDelta / timeDelta) * 1000; // bytes per second

      if (instantSpeed >= 0) {
        this.speedSamples.push(instantSpeed);
        // Keep last 5 samples for smoothing
        if (this.speedSamples.length > 5) {
          this.speedSamples.shift();
        }
        // Calculate average speed
        this.state.uploadSpeed = this.speedSamples.reduce((a, b) => a + b, 0) / this.speedSamples.length;
      }

      this.lastSpeedUpdate = now;
      this.lastBytesUploaded = totalUploaded;
    }
  }

  public resetUploadResults() {
    this.state.uploadedFiles = 0;
    this.state.totalFiles = 0;
    this.state.uploadedBytes = 0;
    this.state.totalBytes = 0;
    this.state.uploadSpeed = 0;
    this.state.isDryRun = false;
    this.state.isPaused = false;
    this.state.albumStatus = null;
    this.state.isCreatingAlbum = false;
    this.completedBytes = 0;
    this.fileBytes.clear();
    this.totalBytesAdjustment = 0;
    this.lastBytesUploaded = 0;
    this.speedSamples = [];
    this.state.results.success = [];
    this.state.results.fail = [];
    this.state.results.skipped = [];
    this.state.results.warnings = [];
  }

  public async upload(paths: string[]) {
    if (!paths || paths.length === 0) return;
    if (this.isUploadBlocked()) return;
    this.resetUploadResults();
    this.beginPreparation();
    try {
      await Events.Emit("startUpload", { files: paths, dryRun: false });
    } catch (e) {
      this.endPreparation();
      throw e;
    }
  }

  public async retryFailed(paths?: string[]) {
    const toRetry = paths && paths.length > 0
      ? paths
      : this.state.results.fail.map((f) => (typeof f === "string" ? f : f.path));
    if (toRetry.length === 0) return;
    await this.upload(toRetry);
  }

  public cancelUpload() {
    this.state.isPaused = false;
    Events.Emit("uploadCancel");
  }

  public pauseUpload() {
    this.state.isPaused = true;
    Events.Emit("uploadPause");
  }

  public resumeUpload() {
    this.state.isPaused = false;
    Events.Emit("uploadResume");
  }

  public togglePause() {
    if (this.state.isPaused) {
      this.resumeUpload();
    } else {
      this.pauseUpload();
    }
  }

  public async copyResultsAsJson() {
    const resultsJson = JSON.stringify(this.state.results, null, 2);
    try {
      await Clipboard.SetText(resultsJson);
      console.log("Upload results copied to clipboard");
      return true;
    } catch (error) {
      console.error("Failed to copy results:", error);
      return false;
    }
  }
}

// Create and export a single instance
export const uploadManager = UploadManager.getInstance();
