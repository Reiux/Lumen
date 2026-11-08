//go:build !cli

package main

import (
	"embed"
	"fmt"
	"log"
	"os"
	"runtime"
	"strings"
	"sync"
	"time"

	"app/core"
	"app/internal/cli"
	"app/internal/gui"

	"github.com/wailsapp/wails/v3/pkg/application"
	"github.com/wailsapp/wails/v3/pkg/events"
	"github.com/wailsapp/wails/v3/pkg/services/notifications"
)

//go:embed all:frontend/dist
var assets embed.FS

//go:embed build/trayicon.png
var trayIcon []byte

var title = "Lumen v" + getAppVersion()

func main() {
	// Run the CLI when the arguments name one of its commands; otherwise start the GUI.
	if cli.IsCLIInvocation(os.Args[1:]) {
		os.Exit(cli.Run(os.Args[1:], cli.Info{
			ExecutableName: "lumen",
			HasGUI:         true,
			Version:        getAppVersion(),
		}))
	}

	runGUI()
}

func runGUI() {
	if runtime.GOOS == "linux" && os.Getenv("WEBKIT_DISABLE_DMABUF_RENDERER") == "" {
		_ = os.Setenv("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
	}

	normalizeFrontendDevServerURL()

	configManager := &core.ConfigManager{}
	notificationService := notifications.New()

	var window *application.WebviewWindow

	wailsApp := application.New(application.Options{
		Name:        "Lumen",
		Description: "Google Photos unofficial client",
		Services: []application.Service{
			application.NewService(configManager),
			application.NewService(notificationService),
		},
		Assets: application.AssetOptions{
			Handler: application.BundledAssetFileServer(assets),
		},
		Mac: application.MacOptions{
			ApplicationShouldTerminateAfterLastWindowClosed: false,
		},
		SingleInstance: &application.SingleInstanceOptions{
			UniqueID: "com.reiux.lumen",
			OnSecondInstanceLaunch: func(data application.SecondInstanceData) {
				if window != nil {
					window.Show()
					window.UnMinimise()
					window.Focus()
				}
			},
		},
	})

	bgType := application.BackgroundTypeSolid
	if runtime.GOOS == "darwin" {
		bgType = application.BackgroundTypeTranslucent
	}

	window = wailsApp.Window.NewWithOptions(application.WebviewWindowOptions{
		Title:               title,
		Frameless:           false,
		Width:               1000,
		Height:              680,
		MinWidth:            800,
		MinHeight:           560,
		InitialPosition:     application.WindowCentered,
		EnableFileDrop:      true,
		DisableResize:       false,
		MaximiseButtonState: application.ButtonEnabled,
		BackgroundColour:    application.NewRGB(18, 19, 22),
		BackgroundType:      bgType,
		Hidden:              true,
		Mac: application.MacWindow{
			InvisibleTitleBarHeight: 0,
			Backdrop:                application.MacBackdropTranslucent,
			TitleBar:                application.MacTitleBarHiddenInset,
		},
		URL: "/",
	})

	window.Center()

	var showOnce sync.Once
	showWindow := func() {
		showOnce.Do(func() {
			window.Show()
		})
	}
	// Window starts hidden with native dark background (#121316) to eliminate white flash.
	// It is shown only once the frontend UI mounts and emits "app:ready".
	wailsApp.Event.On("app:ready", func(*application.CustomEvent) {
		showWindow()
	})
	time.AfterFunc(2500*time.Millisecond, showWindow)

	// Detect if system tray is supported on this desktop environment
	traySupported := true
	if runtime.GOOS == "linux" {
		if os.Getenv("DBUS_SESSION_BUS_ADDRESS") == "" && os.Getenv("DISPLAY") == "" && os.Getenv("WAYLAND_DISPLAY") == "" {
			traySupported = false
		}
	}

	tray := wailsApp.SystemTray.New()
	if tray == nil {
		traySupported = false
	}
	core.SetTrayAvailable(traySupported)

	if tray != nil && traySupported {
		tray.SetIcon(trayIcon)
		tray.SetLabel("Lumen")
		tray.SetTooltip("Lumen")
		trayMenu := application.NewMenu()
		trayMenu.Add("Show").OnClick(func(ctx *application.Context) {
			window.Show()
			window.UnMinimise()
			window.Focus()
		})
		trayMenu.AddSeparator()
		trayMenu.Add("Quit").OnClick(func(ctx *application.Context) {
			wailsApp.Quit()
		})
		tray.SetMenu(trayMenu)
		tray.OnClick(func() {
			if window.IsVisible() {
				window.Hide()
			} else {
				window.Show()
				window.UnMinimise()
				window.Focus()
			}
		})
	}

	// Intercept window close if MinimizeToTray preference is enabled and tray is available
	window.RegisterHook(events.Common.WindowClosing, func(event *application.WindowEvent) {
		if configManager.GetSettings().MinimizeToTray && core.TrayAvailable && tray != nil {
			event.Cancel()
			window.Hide()
		}
	})

	// Native folder picker for watching directories and uploads
	core.FolderPickerFunc = func() (string, error) {
		return wailsApp.Dialog.OpenFile().
			CanChooseDirectories(true).
			CanChooseFiles(false).
			AttachToWindow(window).
			SetTitle("Select Folder").
			PromptForSingleSelection()
	}

	// Native multi-file picker for uploads
	core.FilePickerFunc = func() ([]string, error) {
		return wailsApp.Dialog.OpenFile().
			CanChooseDirectories(false).
			CanChooseFiles(true).
			AttachToWindow(window).
			SetTitle("Select Photos & Videos to Upload").
			PromptForMultipleSelection()
	}

	core.BrowserCloseFailedFunc = func() {
		wailsApp.Event.Emit("browser-signin:close-failed", nil)
	}

	uploadManager := core.NewUploadManager(gui.NewWailsReporter(wailsApp), wailsApp.Logger)

	// Background Folder Watcher integration
	var (
		watchMu           sync.Mutex
		pendingWatchFiles []string
	)

	dispatchWatchedBatch := func(batch []string) {
		opts := configManager.SessionUploadOptions()
		opts.DeleteFromHost = false // Watcher NEVER deletes local files
		if len(batch) > 0 {
			targetAccount := configManager.GetAccountForPath(batch[0])
			if targetAccount != "" {
				opts.AccountEmail = targetAccount
				opts.Api.Account = targetAccount
			}
			folderCfg := configManager.GetWatchedFolderConfigForPath(batch[0])
			if folderCfg != nil {
				if folderCfg.AlbumMode == "auto" {
					opts.AlbumAutoMode = true
					opts.AlbumName = ""
				} else if folderCfg.AlbumMode == "custom" && folderCfg.AlbumNameOrKey != "" {
					opts.AlbumAutoMode = false
					opts.AlbumName = folderCfg.AlbumNameOrKey
				} else if folderCfg.AlbumMode == "none" {
					opts.AlbumAutoMode = false
					opts.AlbumName = ""
				}
			}
		}
		wailsApp.Event.Emit("folder-watch:detected", map[string]any{
			"files": batch,
			"count": len(batch),
		})
		uploadManager.Upload(batch, opts)
	}

	onWatchedFiles := func(files []string) {
		watchMu.Lock()
		defer watchMu.Unlock()
		if uploadManager.IsRunning() {
			pendingWatchFiles = append(pendingWatchFiles, files...)
			return
		}
		wailsApp.Logger.Info("Watched folder detected new files", "count", len(files))
		dispatchWatchedBatch(files)
	}

	_ = core.InitFolderWatcher(onWatchedFiles)

	// Check for updates in background if AutoCheckUpdates is enabled
	go func() {
		time.Sleep(3 * time.Second)
		updateInfo, err := configManager.CheckForUpdates(false)
		if err == nil && updateInfo.HasUpdate {
			wailsApp.Event.Emit("app:update-available", updateInfo)
		}
	}()

	// Listen for upload completion to drain queued watch files or send desktop notification
	wailsApp.Event.On("uploadStop", func(e *application.CustomEvent) {
		watchMu.Lock()
		if len(pendingWatchFiles) > 0 {
			nextBatch := pendingWatchFiles
			pendingWatchFiles = nil
			watchMu.Unlock()
			go func() {
				time.Sleep(500 * time.Millisecond)
				wailsApp.Logger.Info("Processing queued watched files", "fileCount", len(nextBatch))
				dispatchWatchedBatch(nextBatch)
			}()
			return
		}
		watchMu.Unlock()

		if configManager.GetSettings().NotifyOnComplete {
			// Send one summary notification per finished batch without filenames. Fail quietly if daemon unavailable.
			if err := notificationService.SendNotification(notifications.NotificationOptions{
				ID:    fmt.Sprintf("upload-complete-%d", time.Now().UnixNano()),
				Title: "Lumen",
				Body:  "Upload batch complete",
			}); err != nil {
				wailsApp.Logger.Debug("Notification daemon unavailable or failed quietly", "err", err)
			}
		}
	})

	// Listen for upload cancel event
	wailsApp.Event.On("uploadCancel", func(e *application.CustomEvent) {
		uploadManager.Cancel()
	})

	// Developer tool event: test desktop notification
	wailsApp.Event.On("dev:test-notification", func(e *application.CustomEvent) {
		if err := notificationService.SendNotification(notifications.NotificationOptions{
			ID:    fmt.Sprintf("test-notification-%d", time.Now().UnixNano()),
			Title: "Lumen",
			Body:  "Upload batch complete (test notification)",
		}); err != nil {
			wailsApp.Logger.Debug("Test notification failed", "err", err)
		}
	})

	window.OnWindowEvent(events.Common.WindowFilesDropped, func(event *application.WindowEvent) {
		paths := event.Context().DroppedFiles()
		dropTarget := event.Context().DropTargetDetails()

		var dropZone string
		if dropTarget != nil {
			dropZone = dropTarget.Attributes["data-drop-zone"]
			wailsApp.Logger.Info("Drop target detected",
				"dropZone", dropZone,
				"elementID", dropTarget.ElementID)
		}

		// Emit event to frontend with drop details
		wailsApp.Event.Emit("files-dropped", core.FilesDroppedEvent{
			Files:    paths,
			DropZone: dropZone,
		})
	})

	// Listen for upload request from frontend (after drop zone is determined)
	wailsApp.Event.On("startUpload", func(e *application.CustomEvent) {
		if data, ok := e.Data.(core.StartUploadEvent); ok {
			opts := configManager.SessionUploadOptions()
			if len(data.Files) > 0 {
				folderCfg := configManager.GetWatchedFolderConfigForPath(data.Files[0])
				if folderCfg != nil {
					if folderCfg.AccountEmail != "" {
						opts.AccountEmail = folderCfg.AccountEmail
						opts.Api.Account = folderCfg.AccountEmail
					}
					if folderCfg.AlbumMode == "auto" {
						opts.AlbumAutoMode = true
						opts.AlbumName = ""
					} else if folderCfg.AlbumMode == "custom" && folderCfg.AlbumNameOrKey != "" {
						opts.AlbumAutoMode = false
						opts.AlbumName = folderCfg.AlbumNameOrKey
					} else if folderCfg.AlbumMode == "none" {
						opts.AlbumAutoMode = false
						opts.AlbumName = ""
					}
					if folderCfg.IncludeSubfolders {
						opts.Recursive = true
					}
				} else {
					for _, path := range data.Files {
						if info, err := os.Stat(path); err == nil && info.IsDir() {
							opts.Recursive = true
							break
						}
					}
				}
			}
			wailsApp.Logger.Info("Starting upload", "fileCount", len(data.Files), "albumName", opts.AlbumName, "albumAutoMode", opts.AlbumAutoMode)
			uploadManager.Upload(data.Files, opts)
		} else {
			wailsApp.Logger.Error("startUpload: unexpected data type", "type", fmt.Sprintf("%T", e.Data))
		}
	})

	err := wailsApp.Run()
	if err != nil {
		log.Fatal(err)
	}
}

func normalizeFrontendDevServerURL() {
	const envName = "FRONTEND_DEVSERVER_URL"

	value := os.Getenv(envName)
	value = strings.Replace(value, "http://localhost:", "http://127.0.0.1:", 1)
	value = strings.Replace(value, "https://localhost:", "https://127.0.0.1:", 1)
	if value != os.Getenv(envName) {
		_ = os.Setenv(envName, value)
	}
}
