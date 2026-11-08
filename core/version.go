package core

import (
	"embed"
	"encoding/json"
)

// AppVersion is the canonical semantic version of Lumen Desktop.
const AppVersion = "1.1.0"

type VersionInfo struct {
	Fixed struct {
		FileVersion string `json:"file_version"`
	} `json:"fixed"`
}

func GetVersion(content embed.FS) string {
	// Read the embedded file
	data, err := content.ReadFile("build/windows/info.json")
	if err != nil {
		return AppVersion
	}

	// Parse the JSON
	var versionInfo VersionInfo
	err = json.Unmarshal(data, &versionInfo)
	if err != nil || versionInfo.Fixed.FileVersion == "" {
		return AppVersion
	}
	return versionInfo.Fixed.FileVersion
}
