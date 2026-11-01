package core

import (
	"testing"
)

func TestExtractOAuthToken(t *testing.T) {
	const fakeToken = "FAKE123"

	tests := []struct {
		name      string
		input     string
		wantToken string
		wantFound bool
	}{
		// 1. URL Query Form
		{
			name:      "URL query parameter oauth_token",
			input:     "https://accounts.google.com/EmbeddedSetup?oauth_token=FAKE123",
			wantToken: fakeToken,
			wantFound: true,
		},
		{
			name:      "URL query with multiple parameters",
			input:     "https://accounts.google.com/callback?foo=bar&oauth_token=FAKE123&baz=456",
			wantToken: fakeToken,
			wantFound: true,
		},
		{
			name:      "URL query with Token title case",
			input:     "https://accounts.google.com/callback?Token=FAKE123",
			wantToken: fakeToken,
			wantFound: true,
		},
		{
			name:      "URL fragment with oauth_token",
			input:     "https://accounts.google.com/callback#oauth_token=FAKE123",
			wantToken: fakeToken,
			wantFound: true,
		},
		{
			name:      "Bare query string",
			input:     "?oauth_token=FAKE123&other=val",
			wantToken: fakeToken,
			wantFound: true,
		},

		// 2. Cookie-Header Form
		{
			name:      "Cookie header simple",
			input:     "oauth_token=FAKE123",
			wantToken: fakeToken,
			wantFound: true,
		},
		{
			name:      "Cookie header with attributes",
			input:     "oauth_token=FAKE123; Path=/; Secure; HttpOnly",
			wantToken: fakeToken,
			wantFound: true,
		},
		{
			name:      "Cookie header among other cookies",
			input:     "SID=some_session; oauth_token=FAKE123; HSID=some_hsid",
			wantToken: fakeToken,
			wantFound: true,
		},
		{
			name:      "Cookie header quoted value",
			input:     `oauth_token="FAKE123"; Path=/`,
			wantToken: fakeToken,
			wantFound: true,
		},

		// 3. Missing Token
		{
			name:      "URL without token",
			input:     "https://accounts.google.com/EmbeddedSetup",
			wantToken: "",
			wantFound: false,
		},
		{
			name:      "Cookie header without oauth_token",
			input:     "SID=some_session; HSID=some_hsid",
			wantToken: "",
			wantFound: false,
		},
		{
			name:      "Similar parameter name but not oauth_token",
			input:     "https://accounts.google.com/callback?oauth_other=FAKE123",
			wantToken: "",
			wantFound: false,
		},

		// 4. Empty Input / Whitespace
		{
			name:      "Empty string",
			input:     "",
			wantToken: "",
			wantFound: false,
		},
		{
			name:      "Whitespace only",
			input:     "   \t\n  ",
			wantToken: "",
			wantFound: false,
		},
		{
			name:      "Empty token parameter",
			input:     "https://accounts.google.com/callback?oauth_token=",
			wantToken: "",
			wantFound: false,
		},
		{
			name:      "Empty cookie value",
			input:     "oauth_token=",
			wantToken: "",
			wantFound: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			gotToken, gotFound := extractOAuthToken(tt.input)
			if gotFound != tt.wantFound {
				t.Fatalf("extractOAuthToken() found = %v, want %v", gotFound, tt.wantFound)
			}
			if gotToken != tt.wantToken {
				t.Fatalf("extractOAuthToken() token = %v, want %v", gotToken, tt.wantToken)
			}
		})
	}
}
