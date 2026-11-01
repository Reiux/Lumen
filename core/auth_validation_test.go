package core

import (
	"errors"
	"strings"
	"testing"
)

func TestClassifyAuthError(t *testing.T) {
	tests := []struct {
		name     string
		input    error
		expected string
	}{
		{
			name:     "nil error",
			input:    nil,
			expected: "",
		},
		{
			name:     "BadAuthentication",
			input:    errors.New("request failed: BadAuthentication"),
			expected: "Token expired or rejected by Google",
		},
		{
			name:     "HTTP 401 Unauthorized",
			input:    errors.New("Google authentication returned HTTP 401 unauthorized"),
			expected: "Token expired or rejected by Google",
		},
		{
			name:     "NeedsBrowser challenge",
			input:    errors.New("Google authentication failed: NeedsBrowser"),
			expected: "Google requires browser verification",
		},
		{
			name:     "AccountDisabled",
			input:    errors.New("Google auth error: AccountDisabled"),
			expected: "This Google account has been disabled",
		},
		{
			name:     "Network timeout",
			input:    errors.New("context deadline exceeded (Client.Timeout or network timeout)"),
			expected: "Connection timed out",
		},
		{
			name:     "Connection refused / offline",
			input:    errors.New("dial tcp 127.0.0.1: connection refused"),
			expected: "Connection failed",
		},
		{
			name:     "Format error",
			input:    errors.New("missing required field in auth format"),
			expected: "Invalid credential format",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := classifyAuthError(tt.input)
			if tt.input == nil {
				if got != nil {
					t.Errorf("expected nil, got %v", got)
				}
				return
			}
			if got == nil {
				t.Fatalf("expected error, got nil")
			}
			if !strings.Contains(got.Error(), tt.expected) {
				t.Errorf("expected error containing %q, got %q", tt.expected, got.Error())
			}
		})
	}
}

func TestValidateAndAddCredentials_Validation(t *testing.T) {
	mgr := &ConfigManager{}

	// 1. Empty input
	_, err := mgr.ValidateAndAddCredentials("")
	if err == nil {
		t.Fatal("expected error for empty credentials")
	}

	// 2. Whitespace only
	_, err = mgr.ValidateAndAddCredentials("   ")
	if err == nil {
		t.Fatal("expected error for whitespace credentials")
	}

	// 3. Invalid format with missing token
	_, err = mgr.ValidateAndAddCredentials("Email=test@example.com")
	if err == nil {
		t.Fatal("expected error for missing token")
	}

	// 4. Invalid format with missing email
	_, err = mgr.ValidateAndAddCredentials("Token=secret123")
	if err == nil {
		t.Fatal("expected error for missing email")
	}
}
