package core

import (
	"net/url"
	"regexp"
	"strings"
)

var (
	// urlTokenRegex matches oauth_token or Token parameter in URL query or fragment.
	urlTokenRegex = regexp.MustCompile(`(?i)[?&#](?:oauth_token|token)=([^&#;\s]+)`)

	// cookieTokenRegex matches oauth_token or Token in HTTP Cookie headers.
	cookieTokenRegex = regexp.MustCompile(`(?i)(?:^|;\s*)(?:oauth_token|token)=([^;\s]+)`)
)

// extractOAuthToken parses a raw URL or Cookie header string and extracts
// the oauth_token (or Token) value if present.
// Returns the extracted token string and a boolean indicating whether it was found.
func extractOAuthToken(rawURLOrCookieHeader string) (string, bool) {
	trimmed := strings.TrimSpace(rawURLOrCookieHeader)
	if trimmed == "" {
		return "", false
	}

	// 1. If it looks like a URL with query or fragment, try standard URL parsing
	if strings.Contains(trimmed, "://") || strings.HasPrefix(trimmed, "/") {
		if parsedURL, err := url.Parse(trimmed); err == nil {
			if qVal := parsedURL.Query().Get("oauth_token"); qVal != "" {
				return qVal, true
			}
			if qVal := parsedURL.Query().Get("Token"); qVal != "" {
				return qVal, true
			}
			if parsedURL.Fragment != "" {
				if fragValues, err := url.ParseQuery(parsedURL.Fragment); err == nil {
					if fVal := fragValues.Get("oauth_token"); fVal != "" {
						return fVal, true
					}
					if fVal := fragValues.Get("Token"); fVal != "" {
						return fVal, true
					}
				}
			}
		}
	}

	// 2. Try URL query/fragment regex matching (matching Android's [?&#](?:oauth_token|Token)=([^&#]+))
	if matches := urlTokenRegex.FindStringSubmatch(trimmed); len(matches) > 1 {
		val := strings.TrimSpace(matches[1])
		val = strings.Trim(val, `"'`)
		if val != "" {
			if unescaped, err := url.QueryUnescape(val); err == nil {
				return unescaped, true
			}
			return val, true
		}
	}

	// 3. Try Cookie header format
	if matches := cookieTokenRegex.FindStringSubmatch(trimmed); len(matches) > 1 {
		val := strings.TrimSpace(matches[1])
		val = strings.Trim(val, `"'`)
		if val != "" {
			return val, true
		}
	}

	// 4. Fallback for raw "oauth_token=VALUE" string without prefix
	if strings.HasPrefix(trimmed, "oauth_token=") || strings.HasPrefix(trimmed, "Token=") {
		parts := strings.SplitN(trimmed, "=", 2)
		if len(parts) == 2 {
			val := strings.TrimSpace(parts[1])
			if idx := strings.Index(val, ";"); idx != -1 {
				val = strings.TrimSpace(val[:idx])
			}
			val = strings.Trim(val, `"'`)
			if val != "" {
				return val, true
			}
		}
	}

	return "", false
}

// ExtractOAuthToken is an exported alias for extractOAuthToken.
func ExtractOAuthToken(rawURLOrCookieHeader string) (string, bool) {
	return extractOAuthToken(rawURLOrCookieHeader)
}
