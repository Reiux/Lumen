//go:build !cli

package gui

import (
	"sync"

	"app/core"
	"github.com/wailsapp/wails/v3/pkg/application"
)

// WailsReporter forwards upload progress to the frontend as Wails events.
type WailsReporter struct {
	app  *application.App
	mu   sync.Mutex
	seen map[string]struct{}
}

func NewWailsReporter(app *application.App) *WailsReporter {
	r := &WailsReporter{
		app:  app,
		seen: make(map[string]struct{}),
	}
	core.GetGlobalLogger().AddListener(func(entry core.LogEntry) {
		r.LogEntry(entry)
	})
	return r
}

func (w *WailsReporter) UploadStart(s core.UploadBatchStart) { w.app.Event.Emit("uploadStart", s) }
func (w *WailsReporter) UploadStop()                         { w.app.Event.Emit("uploadStop", nil) }
func (w *WailsReporter) TotalBytes(n int64)                  { w.app.Event.Emit("uploadTotalBytes", n) }
func (w *WailsReporter) TotalBytesDelta(n int64)             { w.app.Event.Emit("uploadTotalBytesDelta", n) }
func (w *WailsReporter) Warning(p core.PreflightWarning)     { w.app.Event.Emit("uploadWarning", p) }
func (w *WailsReporter) ThreadStatus(s core.ThreadStatus)    { w.app.Event.Emit("ThreadStatus", s) }
func (w *WailsReporter) FileResult(r core.FileUploadResult)  { w.app.Event.Emit("FileStatus", r) }
func (w *WailsReporter) AlbumProgress(s core.AlbumStatus)    { w.app.Event.Emit("albumProgress", s) }
func (w *WailsReporter) AlbumComplete(s core.AlbumStatus)    { w.app.Event.Emit("albumComplete", s) }
func (w *WailsReporter) AlbumError(e core.AlbumError)        { w.app.Event.Emit("albumError", e) }
func (w *WailsReporter) AccountBlocked(e core.AccountBlockedEvent) { w.app.Event.Emit("account:blocked", e) }
func (w *WailsReporter) LogEntry(e core.LogEntry) {
	if e.ID != "" {
		w.mu.Lock()
		if w.seen == nil {
			w.seen = make(map[string]struct{})
		}
		if _, exists := w.seen[e.ID]; exists {
			w.mu.Unlock()
			return
		}
		if len(w.seen) > 2000 {
			w.seen = make(map[string]struct{})
		}
		w.seen[e.ID] = struct{}{}
		w.mu.Unlock()
	}
	w.app.Event.Emit("log:entry", e)
}
func (w *WailsReporter) ScanProgress(e core.ScanProgressEvent) { w.app.Event.Emit("scan:progress", e) }
func (w *WailsReporter) ScanCancelled()                        { w.app.Event.Emit("scan:cancelled", nil) }
