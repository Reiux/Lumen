//go:build !production

package core

func isDebugBuild() bool {
	return true
}
