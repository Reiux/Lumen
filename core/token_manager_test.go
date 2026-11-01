package core

import (
	"context"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestTokenManager_ConcurrentThunderingHerd(t *testing.T) {
	var networkCalls int64

	mockFetcher := func(ctx context.Context, email string) (string, time.Time, error) {
		// Simulate network latency
		time.Sleep(50 * time.Millisecond)
		atomic.AddInt64(&networkCalls, 1)
		// Return token valid for 1 hour
		return "bearer-token-123", time.Now().Add(1 * time.Hour), nil
	}

	tm := NewTokenManager("user@example.com", mockFetcher)

	const workerCount = 10
	var wg sync.WaitGroup
	wg.Add(workerCount)

	tokens := make([]string, workerCount)
	errs := make([]error, workerCount)

	// Launch concurrent requests simultaneously
	for i := 0; i < workerCount; i++ {
		go func(idx int) {
			defer wg.Done()
			token, err := tm.GetToken(context.Background())
			tokens[idx] = token
			errs[idx] = err
		}(i)
	}

	wg.Wait()

	// 1. Verify all workers got the exact same valid token without error
	for i := 0; i < workerCount; i++ {
		if errs[i] != nil {
			t.Fatalf("worker %d returned error: %v", i, errs[i])
		}
		if tokens[i] != "bearer-token-123" {
			t.Fatalf("worker %d got unexpected token: %q", i, tokens[i])
		}
	}

	// 2. CRITICAL: Only 1 network refresh call must have occurred!
	calls := atomic.LoadInt64(&networkCalls)
	if calls != 1 {
		t.Fatalf("expected exactly 1 network call, got %d", calls)
	}
}

func TestTokenManager_FiveMinuteSafetyMargin(t *testing.T) {
	var networkCalls int64

	mockFetcher := func(ctx context.Context, email string) (string, time.Time, error) {
		count := atomic.AddInt64(&networkCalls, 1)
		if count == 1 {
			// Token expiring in 4 minutes (inside the 5-minute safety margin)
			return "token-soon-expiring", time.Now().Add(4 * time.Minute), nil
		}
		// Second call returns fresh token
		return "fresh-token", time.Now().Add(1 * time.Hour), nil
	}

	tm := NewTokenManager("user@example.com", mockFetcher)

	// First call gets token-soon-expiring
	tok1, err := tm.GetToken(context.Background())
	if err != nil {
		t.Fatalf("first GetToken failed: %v", err)
	}
	if tok1 != "token-soon-expiring" {
		t.Fatalf("expected token-soon-expiring, got %q", tok1)
	}

	// Second call should detect it is within 5 minutes of expiry and refresh immediately
	tok2, err := tm.GetToken(context.Background())
	if err != nil {
		t.Fatalf("second GetToken failed: %v", err)
	}
	if tok2 != "fresh-token" {
		t.Fatalf("expected fresh-token due to 5-min margin, got %q", tok2)
	}

	calls := atomic.LoadInt64(&networkCalls)
	if calls != 2 {
		t.Fatalf("expected 2 network calls, got %d", calls)
	}
}

func TestTokenManager_InvalidateToken(t *testing.T) {
	var networkCalls int64

	mockFetcher := func(ctx context.Context, email string) (string, time.Time, error) {
		count := atomic.AddInt64(&networkCalls, 1)
		return "token-" + string(rune('0'+count)), time.Now().Add(1 * time.Hour), nil
	}

	tm := NewTokenManager("user@example.com", mockFetcher)

	tok1, err := tm.GetToken(context.Background())
	if err != nil || tok1 != "token-1" {
		t.Fatalf("unexpected tok1: %q, err: %v", tok1, err)
	}

	// Next immediate call should hit cache
	tokCached, err := tm.GetToken(context.Background())
	if err != nil || tokCached != "token-1" {
		t.Fatalf("unexpected cached token: %q, err: %v", tokCached, err)
	}
	if atomic.LoadInt64(&networkCalls) != 1 {
		t.Fatalf("expected 1 call before invalidate")
	}

	// Invalidate token
	tm.InvalidateToken("user@example.com")

	// Next call must refresh
	tok2, err := tm.GetToken(context.Background())
	if err != nil || tok2 != "token-2" {
		t.Fatalf("unexpected tok2 after invalidate: %q, err: %v", tok2, err)
	}
	if atomic.LoadInt64(&networkCalls) != 2 {
		t.Fatalf("expected 2 calls after invalidate")
	}
}
