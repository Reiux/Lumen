package core

import (
	"context"
	"io"
	"sync"
	"time"
)

// GlobalRateLimiter implements a token-bucket rate limiter that is shared across
// all upload threads and can be updated live at runtime.
type GlobalRateLimiter struct {
	mu          sync.Mutex
	bytesPerSec int64 // 0 means unlimited
	tokens      float64
	capacity    float64
	lastRefill  time.Time
}

var (
	globalLimiter = NewGlobalRateLimiter(0)
)

// NewGlobalRateLimiter creates a new rate limiter with the given bytes per second.
func NewGlobalRateLimiter(bytesPerSec int64) *GlobalRateLimiter {
	l := &GlobalRateLimiter{
		lastRefill: time.Now(),
	}
	l.SetLimit(bytesPerSec)
	return l
}

// SetLimit updates the global bandwidth limit live without restarting uploads.
func (l *GlobalRateLimiter) SetLimit(bytesPerSec int64) {
	l.mu.Lock()
	defer l.mu.Unlock()

	l.bytesPerSec = bytesPerSec
	if bytesPerSec <= 0 {
		l.tokens = 0
		l.capacity = 0
		return
	}
	cap := float64(bytesPerSec) / 2
	if cap < 1024 {
		cap = 1024
	}
	if cap > 64*1024 {
		cap = 64 * 1024
	}
	l.capacity = cap
	initialTokens := float64(bytesPerSec) / 10
	if initialTokens < 512 {
		initialTokens = 512
	}
	if l.tokens > initialTokens || l.tokens <= 0 {
		l.tokens = initialTokens
	}
	l.lastRefill = time.Now()
}

// GetLimit returns the current bandwidth limit in bytes per second.
func (l *GlobalRateLimiter) GetLimit() int64 {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.bytesPerSec
}

func (l *GlobalRateLimiter) refillLocked() {
	now := time.Now()
	elapsed := now.Sub(l.lastRefill).Seconds()
	l.lastRefill = now

	if l.bytesPerSec > 0 && elapsed > 0 {
		l.tokens += elapsed * float64(l.bytesPerSec)
		if l.tokens > l.capacity {
			l.tokens = l.capacity
		}
	}
}

// Wait blocks until n bytes are permitted by the token bucket, or ctx is cancelled.
// It wakes up instantly when the context is cancelled.
func (l *GlobalRateLimiter) Wait(ctx context.Context, n int) error {
	for {
		l.mu.Lock()
		if l.bytesPerSec <= 0 {
			l.mu.Unlock()
			return nil
		}

		l.refillLocked()

		needed := float64(n)
		if l.tokens >= needed {
			l.tokens -= needed
			l.mu.Unlock()
			return nil
		}

		deficit := needed - l.tokens
		waitDuration := time.Duration((deficit / float64(l.bytesPerSec)) * float64(time.Second))
		l.mu.Unlock()

		select {
		case <-time.After(waitDuration):
		case <-ctx.Done():
			return ctx.Err()
		}
	}
}

// SetGlobalRateLimitKBps sets the shared global limit in KB/s.
func SetGlobalRateLimitKBps(kbps int) {
	if kbps <= 0 {
		globalLimiter.SetLimit(0)
	} else {
		globalLimiter.SetLimit(int64(kbps) * 1024)
	}
}

// GetGlobalRateLimiter returns the shared rate limiter singleton.
func GetGlobalRateLimiter() *GlobalRateLimiter {
	return globalLimiter
}

// RateLimitedReader wraps an io.Reader and restricts its throughput using the
// shared GlobalRateLimiter. It is context-aware and terminates immediately if
// the context is cancelled.
type RateLimitedReader struct {
	ctx     context.Context
	r       io.Reader
	limiter *GlobalRateLimiter
}

// NewRateLimitedReader wraps r to pace reads through the shared GlobalRateLimiter.
func NewRateLimitedReader(ctx context.Context, r io.Reader) io.Reader {
	if ctx == nil {
		ctx = context.Background()
	}
	return &RateLimitedReader{
		ctx:     ctx,
		r:       r,
		limiter: globalLimiter,
	}
}

// Read reads from the underlying reader and paces execution through the global rate limiter.
func (lr *RateLimitedReader) Read(p []byte) (int, error) {
	limit := lr.limiter.GetLimit()
	if limit > 0 {
		maxChunk := int(limit / 10)
		if maxChunk < 1024 {
			maxChunk = 1024
		}
		if maxChunk > 32*1024 {
			maxChunk = 32 * 1024
		}
		if len(p) > maxChunk {
			p = p[:maxChunk]
		}
	}

	n, err := lr.r.Read(p)
	if n > 0 && limit > 0 {
		if waitErr := lr.limiter.Wait(lr.ctx, n); waitErr != nil {
			return n, waitErr
		}
	}

	return n, err
}
