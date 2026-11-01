package core

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestErrorClassifier_StatusCodesAndFalsePositivePrevention(t *testing.T) {
	tests := []struct {
		name     string
		err      error
		expected ErrorKind
	}{
		// True status codes
		{
			name:     "Status 401 Unauthorized",
			err:      fmt.Errorf("request failed with status 401: Unauthorized"),
			expected: ErrorKindSignInRequired,
		},
		{
			name:     "Status 403 Forbidden",
			err:      fmt.Errorf("request failed with status 403: Forbidden"),
			expected: ErrorKindSignInRequired,
		},
		{
			name:     "Status 429 Too Many Requests",
			err:      fmt.Errorf("request failed with status 429: Too Many Requests"),
			expected: ErrorKindRateLimited,
		},
		{
			name:     "Status 500 Internal Server Error",
			err:      fmt.Errorf("request failed with status 500: Server Error"),
			expected: ErrorKindTransient,
		},
		{
			name:     "Status 503 Service Unavailable",
			err:      fmt.Errorf("request failed with status 503: Service Unavailable"),
			expected: ErrorKindTransient,
		},
		// CRITICAL: Filenames and sizes with numbers must NOT trigger false auth failures!
		{
			name:     "Filename containing 4012 - IMG_4012.jpg",
			err:      fmt.Errorf("error reading file /path/to/IMG_4012.jpg: permission denied"),
			expected: ErrorKindFatal,
		},
		{
			name:     "Filename containing 401 - IMG_401.jpg",
			err:      fmt.Errorf("error reading file /path/to/IMG_401.jpg: file not found"),
			expected: ErrorKindFatal,
		},
		{
			name:     "Filename containing 403 - photo_403.heic",
			err:      fmt.Errorf("stat /home/user/photo_403.heic: no such file"),
			expected: ErrorKindFatal,
		},
		{
			name:     "Filename containing 429 - video_429.mp4",
			err:      fmt.Errorf("read video_429.mp4 failed: broken pipe"),
			expected: ErrorKindTransient, // "broken pipe" is transient network
		},
		{
			name:     "File size with 401 bytes - 401000 bytes",
			err:      fmt.Errorf("buffer overflow processing 401000 bytes"),
			expected: ErrorKindFatal,
		},
		// Known Google Auth error keywords
		{
			name:     "NeedsBrowser keyword",
			err:      fmt.Errorf("Google authentication failed with NeedsBrowser"),
			expected: ErrorKindSignInRequired,
		},
		{
			name:     "BadAuthentication keyword",
			err:      fmt.Errorf("Google authentication failed with BadAuthentication"),
			expected: ErrorKindSignInRequired,
		},
		{
			name:     "AccountDisabled keyword",
			err:      fmt.Errorf("Google authentication failed with AccountDisabled"),
			expected: ErrorKindAccountRestricted,
		},
		{
			name:     "AccountDeleted keyword",
			err:      fmt.Errorf("Google authentication failed with AccountDeleted"),
			expected: ErrorKindAccountRestricted,
		},
		{
			name:     "DeviceManagementRequired keyword",
			err:      fmt.Errorf("Google authentication failed with DeviceManagementRequired"),
			expected: ErrorKindAccountRestricted,
		},
		// Transient network errors
		{
			name:     "Connection reset by peer",
			err:      fmt.Errorf("read tcp 192.168.1.5:443: connection reset by peer"),
			expected: ErrorKindTransient,
		},
		{
			name:     "i/o timeout",
			err:      fmt.Errorf("dial tcp 172.217.16.202:443: i/o timeout"),
			expected: ErrorKindTransient,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			kind := ClassifyError(tc.err)
			if kind != tc.expected {
				t.Errorf("expected %s, got %s for error: %v", tc.expected, kind, tc.err)
			}
		})
	}
}

func TestCircuitBreaker_TripsAndCancelsWorkersPreservingQueue(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	var blockedEvent atomic.Value
	var eventFired int64

	cb := NewCircuitBreaker(cancel, func(event AccountBlockedEvent) {
		atomic.AddInt64(&eventFired, 1)
		blockedEvent.Store(event)
	})

	const totalItems = 20
	queue := make(chan int, totalItems)
	for i := 1; i <= totalItems; i++ {
		queue <- i
	}
	close(queue)

	var (
		processedItems []int
		mu             sync.Mutex
		wg             sync.WaitGroup
	)

	const workerCount = 4
	wg.Add(workerCount)

	for w := 0; w < workerCount; w++ {
		go func(workerID int) {
			defer wg.Done()
			for {
				select {
				case <-ctx.Done():
					return
				case item, ok := <-queue:
					if !ok {
						return
					}
					// Simulate item 5 hitting SIGN_IN_REQUIRED
					if item == 5 {
						authErr := fmt.Errorf("request failed with status 401: Unauthorized")
						kind := ClassifyError(authErr)
						cb.Trip(kind, "victim@gmail.com", authErr.Error())
						// Item 5 was in-flight when tripped: do NOT process as completed or failed
						return
					}

					// Normal processing
					time.Sleep(10 * time.Millisecond)
					mu.Lock()
					processedItems = append(processedItems, item)
					mu.Unlock()
				}
			}
		}(w)
	}

	wg.Wait()

	// 1. Breaker must be in BLOCKED state
	if !cb.IsBlocked() {
		t.Fatalf("expected breaker to be BLOCKED, got %s", cb.State())
	}

	// 2. Exactly one alert event emitted
	if atomic.LoadInt64(&eventFired) != 1 {
		t.Fatalf("expected 1 event fired, got %d", atomic.LoadInt64(&eventFired))
	}
	event := blockedEvent.Load().(AccountBlockedEvent)
	if event.Reason != string(ErrorKindSignInRequired) || event.Email != "victim@gmail.com" {
		t.Fatalf("unexpected event payload: %+v", event)
	}

	// 3. Not all items were processed: remaining queue items were preserved!
	mu.Lock()
	countProcessed := len(processedItems)
	mu.Unlock()

	if countProcessed >= totalItems {
		t.Fatalf("expected workers to stop early, but processed all %d items", countProcessed)
	}
}

func TestCircuitBreaker_ParseRetryAfter(t *testing.T) {
	// 1. Seconds format
	d1 := ParseRetryAfter("120")
	if d1 != 120*time.Second {
		t.Errorf("expected 120s, got %v", d1)
	}

	// 2. Missing or empty header defaults to 15m
	d2 := ParseRetryAfter("")
	if d2 != 15*time.Minute {
		t.Errorf("expected 15m, got %v", d2)
	}

	// 3. Invalid format defaults to 15m
	d3 := ParseRetryAfter("invalid-format")
	if d3 != 15*time.Minute {
		t.Errorf("expected 15m, got %v", d3)
	}
}

func TestCircuitBreaker_RetryPolicyStrictness(t *testing.T) {
	// Only TRANSIENT is retryable
	if !IsRetryable(errors.New("i/o timeout")) {
		t.Errorf("i/o timeout should be retryable")
	}
	if !IsRetryable(errors.New("request failed with status 503: Service Unavailable")) {
		t.Errorf("503 should be retryable")
	}

	// Never retry SIGN_IN_REQUIRED or ACCOUNT_RESTRICTED
	if IsRetryable(errors.New("request failed with status 401: Unauthorized")) {
		t.Errorf("401 must NEVER be retryable")
	}
	if IsRetryable(errors.New("request failed with status 403: Forbidden")) {
		t.Errorf("403 must NEVER be retryable")
	}
	if IsRetryable(errors.New("Google authentication failed with NeedsBrowser")) {
		t.Errorf("NeedsBrowser must NEVER be retryable")
	}
	if IsRetryable(errors.New("Google authentication failed with AccountDisabled")) {
		t.Errorf("AccountDisabled must NEVER be retryable")
	}
}
