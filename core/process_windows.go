//go:build windows

package core

import (
	"os/exec"
	"strconv"
)

func prepareCmdSysProcAttr(cmd *exec.Cmd) {}

func killProcessGroup(pid int) error {
	if pid <= 0 {
		return nil
	}
	return exec.Command("taskkill", "/F", "/T", "/PID", strconv.Itoa(pid)).Run()
}
