//go:build cli

package main

import (
	"os"

	"app/internal/cli"
)

// lumen-cli is built without Wails, so it is CLI only.
func main() {
	os.Exit(cli.Run(os.Args[1:], cli.Info{
		ExecutableName: "lumen-cli",
		HasGUI:         false,
		Version:        getAppVersion(),
	}))
}
