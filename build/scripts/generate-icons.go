//go:build ignore

package main

import (
	"log"
	"os"
	"os/exec"
	"path/filepath"
)

func main() {
	tmpDir := os.TempDir()
	tempIco := filepath.Join(tmpDir, "lumen-temp.ico")
	tempIcns := filepath.Join(tmpDir, "lumen-temp.icns")
	defer func() {
		_ = os.Remove(tempIco)
		_ = os.Remove(tempIcns)
	}()

	// Generate Mac icons from appicon.png (squircle with padding)
	cmd1 := exec.Command("wails3", "generate", "icons", "-input", "appicon.png", "-macfilename", filepath.Join("darwin", "icons.icns"), "-windowsfilename", tempIco)
	cmd1.Stdout = os.Stdout
	cmd1.Stderr = os.Stderr
	if err := cmd1.Run(); err != nil {
		log.Fatalf("generate mac icons: %v", err)
	}

	// Generate Windows icons from appicon-windows.png (full-bleed for taskbar)
	cmd2 := exec.Command("wails3", "generate", "icons", "-input", "appicon-windows.png", "-macfilename", tempIcns, "-windowsfilename", filepath.Join("windows", "icon.ico"))
	cmd2.Stdout = os.Stdout
	cmd2.Stderr = os.Stderr
	if err := cmd2.Run(); err != nil {
		log.Fatalf("generate windows icons: %v", err)
	}
}
