package core

import (
	"net/http"
	"testing"
)

func TestNewHTTPClientWithProxy(t *testing.T) {
	// 1. Empty proxy
	client, err := NewHTTPClientWithProxy("")
	if err != nil {
		t.Fatalf("unexpected error with empty proxy: %v", err)
	}
	if client == nil {
		t.Fatal("expected non-nil client")
	}
	tr, ok := client.Transport.(*http.Transport)
	if !ok || tr == nil {
		t.Fatal("expected Transport to be *http.Transport")
	}
	if tr.TLSClientConfig == nil || tr.TLSClientConfig.InsecureSkipVerify {
		t.Errorf("expected InsecureSkipVerify to be false, got: %v", tr.TLSClientConfig)
	}

	// 2. Valid proxy with whitespace
	clientProxy, err := NewHTTPClientWithProxy("  http://127.0.0.1:8080  ")
	if err != nil {
		t.Fatalf("unexpected error with valid proxy: %v", err)
	}
	if clientProxy == nil {
		t.Fatal("expected non-nil clientProxy")
	}

	// 3. Invalid proxy URL
	_, err = NewHTTPClientWithProxy("://invalid-url")
	if err == nil {
		t.Error("expected error for invalid proxy URL, got nil")
	}
}
