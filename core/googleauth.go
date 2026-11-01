package core

import (
	"context"
	"crypto/rand"
	"crypto/tls"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/mail"
	"net/url"
	"strings"
	"time"
)

const (
	embeddedSetupAuthEndpoint = "https://android.clients.google.com/auth"
	googleAuthEmailHint       = "oauth-token@example.com"
	googlePlayServicesSig     = "38918a453d07199354f8b19af05ec6562ced5788"
	googlePhotosPackage       = "com.google.android.apps.photos"
	googlePhotosSig           = "24bb24c05e47e0aefa68a58a766179d9b613a600"
	googlePhotosService       = "oauth2:openid https://www.googleapis.com/auth/mobileapps.native https://www.googleapis.com/auth/photos.native"
)

type googleAccountAuthDependencies struct {
	client             *http.Client
	authEndpoint       string
	generateAndroidID  func() (string, error)
	validateCredential func(string) error
}

type googleAuthExchange struct {
	Email       string
	MasterToken string
}

// AddGoogleAccount exchanges an Embedded Setup oauth_token for a Google Photos
// credential using the GUI's proxy preference, saves it, and returns the email.
func (g *ConfigManager) AddGoogleAccount(oauthToken string) (string, error) {
	ensureConfigLoaded()

	configMu.RLock()
	proxy := AppConfig.Preferences.Proxy
	configMu.RUnlock()

	return g.AddGoogleAccountWithProxy(oauthToken, proxy)
}

// AddGoogleAccountWithProxy is AddGoogleAccount with an explicit proxy, for
// callers such as the CLI that do not use GUI preferences.
//
//wails:ignore
func (g *ConfigManager) AddGoogleAccountWithProxy(oauthToken string, proxy string) (string, error) {
	client, err := newGoogleAuthHTTPClient(proxy)
	if err != nil {
		return "", fmt.Errorf("prepare Google authentication: %w", err)
	}

	dependencies := googleAccountAuthDependencies{
		client:            client,
		authEndpoint:      embeddedSetupAuthEndpoint,
		generateAndroidID: generateAndroidID,
		validateCredential: func(credential string) error {
			return validateGooglePhotosCredential(credential, proxy)
		},
	}

	return g.addGoogleAccount(context.Background(), oauthToken, dependencies)
}

func (g *ConfigManager) addGoogleAccount(
	ctx context.Context,
	oauthToken string,
	dependencies googleAccountAuthDependencies,
) (string, error) {
	var err error
	oauthToken, err = normalizeOAuthToken(oauthToken)
	if err != nil {
		return "", err
	}

	androidID, err := dependencies.generateAndroidID()
	if err != nil {
		return "", fmt.Errorf("generate Android ID: %w", err)
	}

	exchange, err := exchangeEmbeddedSetupToken(
		ctx,
		dependencies.client,
		dependencies.authEndpoint,
		oauthToken,
		androidID,
	)
	if err != nil {
		return "", err
	}

	credential := buildGooglePhotosCredential(exchange.Email, exchange.MasterToken, androidID)
	if err := dependencies.validateCredential(credential); err != nil {
		return "", fmt.Errorf("Google Photos rejected the new credential: %w", err)
	}

	if err := upsertCredential(credential); err != nil {
		return "", fmt.Errorf("save Google account: %w", err)
	}

	return exchange.Email, nil
}

// ValidateAndAddCredentials validates a direct credential string or Embedded Setup oauth_token,
// confirms connectivity with Google Photos, saves valid credentials to the OS keyring,
// and returns the authenticated user's email.
func (g *ConfigManager) ValidateAndAddCredentials(rawInput string) (string, error) {
	rawInput = strings.TrimSpace(rawInput)
	if rawInput == "" {
		return "", errors.New("credential input cannot be empty")
	}

	ensureConfigLoaded()

	configMu.RLock()
	proxy := AppConfig.Preferences.Proxy
	configMu.RUnlock()

	// 1. If it looks like an auth query string (has Email and Token)
	if LooksLikeAuthString(rawInput) {
		params, err := url.ParseQuery(rawInput)
		if err != nil {
			return "", errors.New("Invalid credential format. Expected key=value parameters.")
		}
		email := strings.TrimSpace(params.Get("Email"))
		token := strings.TrimSpace(params.Get("Token"))
		if email == "" || token == "" {
			return "", errors.New("Invalid credential format. Email and Token are required.")
		}

		androidID := strings.TrimSpace(params.Get("androidId"))
		if androidID == "" {
			androidID, err = generateAndroidID()
			if err != nil {
				return "", fmt.Errorf("generate Android ID: %w", err)
			}
		}

		// Rebuild proper Google Photos credential
		credential := buildGooglePhotosCredential(email, token, androidID)
		// Preserve token binding alias if present
		if alias := params.Get("token_binding_alias"); alias != "" {
			vals, _ := url.ParseQuery(credential)
			vals.Set("token_binding_alias", alias)
			credential = vals.Encode()
		}

		// Immediate live validation probe against Google Photos
		if err := validateGooglePhotosCredential(credential, proxy); err != nil {
			return "", classifyAuthError(err)
		}

		// If validation passed, persist credential
		if err := upsertCredential(credential); err != nil {
			return "", fmt.Errorf("save Google account: %w", err)
		}
		return email, nil
	}

	// 2. Otherwise treat as oauth_token (e.g. from Embedded Setup cookie)
	cleanToken := rawInput
	if strings.Contains(cleanToken, "oauth_token=") {
		parts := strings.Split(cleanToken, "oauth_token=")
		if len(parts) > 1 {
			cleanToken = strings.Fields(parts[1])[0]
			cleanToken = strings.Trim(cleanToken, ";\"'")
		}
	}

	email, err := g.AddGoogleAccountWithProxy(cleanToken, proxy)
	if err != nil {
		return "", classifyAuthError(err)
	}
	return email, nil
}

func classifyAuthError(err error) error {
	if err == nil {
		return nil
	}
	msg := err.Error()
	lower := strings.ToLower(msg)

	if strings.Contains(msg, "BadAuthentication") || strings.Contains(msg, "401") || strings.Contains(lower, "unauthorized") {
		return errors.New("Token expired or rejected by Google. Please obtain a fresh token.")
	}
	if strings.Contains(msg, "NeedsBrowser") || strings.Contains(lower, "needsbrowser") {
		return errors.New("Google requires browser verification for this account.")
	}
	if strings.Contains(msg, "AccountDisabled") || strings.Contains(msg, "AccountDeleted") || strings.Contains(lower, "restricted") {
		return errors.New("This Google account has been disabled or restricted by Google.")
	}
	if strings.Contains(lower, "timeout") || strings.Contains(lower, "deadline") {
		return errors.New("Connection timed out. Please check your network connection.")
	}
	if strings.Contains(lower, "connection refused") || strings.Contains(lower, "no such host") || strings.Contains(lower, "reset by peer") {
		return errors.New("Connection failed. Please check your internet or proxy settings.")
	}
	if strings.Contains(lower, "format") || strings.Contains(lower, "missing") {
		return errors.New("Invalid credential format. Expected Email and Token.")
	}
	return err
}

func newGoogleAuthHTTPClient(proxyURL string) (*http.Client, error) {
	transport := http.DefaultTransport.(*http.Transport).Clone()
	if transport.TLSClientConfig == nil {
		transport.TLSClientConfig = &tls.Config{MinVersion: tls.VersionTLS12}
	} else {
		transport.TLSClientConfig = transport.TLSClientConfig.Clone()
	}
	transport.TLSClientConfig.InsecureSkipVerify = false
	transport.ForceAttemptHTTP2 = true
	transport.TLSNextProto = nil

	if strings.TrimSpace(proxyURL) != "" {
		parsedProxy, err := url.Parse(proxyURL)
		if err != nil {
			return nil, fmt.Errorf("invalid proxy URL: %w", err)
		}
		transport.Proxy = http.ProxyURL(parsedProxy)
	}

	return &http.Client{
		Transport:     transport,
		Timeout:       30 * time.Second,
		CheckRedirect: rejectGoogleAuthRedirect,
	}, nil
}

// These fixed endpoints must not forward token-bearing requests to a redirect
// target. Return the original response instead of a url.Error containing the
// untrusted redirect URL.
func rejectGoogleAuthRedirect(_ *http.Request, _ []*http.Request) error {
	return http.ErrUseLastResponse
}

func exchangeEmbeddedSetupToken(
	ctx context.Context,
	client *http.Client,
	endpoint string,
	oauthToken string,
	androidID string,
) (googleAuthExchange, error) {
	form := url.Values{
		"accountType":                  {"HOSTED_OR_GOOGLE"},
		"Email":                        {googleAuthEmailHint},
		"has_permission":               {"1"},
		"add_account":                  {"1"},
		"ACCESS_TOKEN":                 {"1"},
		"Token":                        {oauthToken},
		"service":                      {"ac2dm"},
		"source":                       {"android"},
		"androidId":                    {androidID},
		"device_country":               {"us"},
		"operatorCountry":              {"us"},
		"lang":                         {"en"},
		"sdk_version":                  {"17"},
		"google_play_services_version": {"240913000"},
		"client_sig":                   {googlePlayServicesSig},
		"callerSig":                    {googlePlayServicesSig},
		"droidguard_results":           {"dummy123"},
	}

	request, err := http.NewRequestWithContext(
		ctx,
		http.MethodPost,
		endpoint,
		strings.NewReader(form.Encode()),
	)
	if err != nil {
		return googleAuthExchange{}, fmt.Errorf("prepare Google token exchange: %w", err)
	}
	request.Header.Set("Accept-Encoding", "identity")
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	request.Header.Set("User-Agent", "GoogleAuth/1.4")

	response, err := client.Do(request)
	if err != nil {
		return googleAuthExchange{}, fmt.Errorf("contact Google authentication: %w", err)
	}
	defer func() { _ = response.Body.Close() }()

	if response.StatusCode < http.StatusOK || response.StatusCode >= http.StatusMultipleChoices {
		return googleAuthExchange{}, fmt.Errorf("Google authentication returned HTTP %d", response.StatusCode)
	}

	body, err := io.ReadAll(io.LimitReader(response.Body, 64*1024))
	if err != nil {
		return googleAuthExchange{}, fmt.Errorf("read Google authentication response: %w", err)
	}
	values := parseGoogleAuthResponse(string(body))
	if authError := values["Error"]; authError != "" {
		return googleAuthExchange{}, googleAuthError(authError)
	}

	masterToken := values["Token"]
	if masterToken == "" {
		return googleAuthExchange{}, errors.New("Google authentication response did not contain a master token")
	}
	email, err := normalizeGoogleEmail(values["Email"])
	if err != nil {
		return googleAuthExchange{}, errors.New("Google authentication response did not contain a valid account email")
	}

	return googleAuthExchange{Email: email, MasterToken: masterToken}, nil
}

func parseGoogleAuthResponse(body string) map[string]string {
	result := make(map[string]string)
	for _, line := range strings.Split(body, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		parts := strings.SplitN(line, "=", 2)
		if len(parts) == 2 {
			result[parts[0]] = parts[1]
		}
	}
	return result
}

func googleAuthError(code string) error {
	switch code {
	case "BadAuthentication":
		return errors.New("Google rejected the oauth_token; obtain a fresh cookie and try again")
	case "NeedsBrowser":
		return errors.New("Google requires a fresh Embedded Setup sign-in")
	case "MissingDroidguard":
		return errors.New("Google rejected the device verification data")
	default:
		return fmt.Errorf("Google authentication failed with %s", code)
	}
}

func normalizeGoogleEmail(value string) (string, error) {
	value = strings.TrimSpace(value)
	address, err := mail.ParseAddress(value)
	if err != nil || address.Address != value {
		return "", errors.New("enter a valid Google account email address")
	}
	return address.Address, nil
}

func normalizeOAuthToken(value string) (string, error) {
	value = strings.TrimSpace(value)
	if strings.HasPrefix(value, "oauth_token=") {
		value = strings.TrimPrefix(value, "oauth_token=")
	}
	if len(value) < 16 || len(value) > 8192 || strings.ContainsAny(value, "\r\n") {
		return "", errors.New("enter the oauth_token cookie value from Google Embedded Setup")
	}
	return value, nil
}

func generateAndroidID() (string, error) {
	raw := make([]byte, 8)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	return hex.EncodeToString(raw), nil
}

func buildGooglePhotosCredential(email string, masterToken string, androidID string) string {
	values := url.Values{
		"androidId":                    {androidID},
		"app":                          {googlePhotosPackage},
		"callerPkg":                    {googlePhotosPackage},
		"callerSig":                    {googlePhotosSig},
		"client_sig":                   {googlePhotosSig},
		"device_country":               {"us"},
		"Email":                        {email},
		"google_play_services_version": {"240913000"},
		"lang":                         {"en_US"},
		"oauth2_foreground":            {"1"},
		"operatorCountry":              {"us"},
		"sdk_version":                  {"33"},
		"service":                      {googlePhotosService},
		"source":                       {"android"},
		"Token":                        {masterToken},
	}
	return values.Encode()
}

func validateGooglePhotosCredential(credential string, proxy string) error {
	api, err := newAPIFromCredential(credential, proxy)
	if err != nil {
		return err
	}
	api.client.CheckRedirect = rejectGoogleAuthRedirect
	if _, err := api.BearerToken(); err != nil {
		return err
	}

	mediaKey, err := api.FindRemoteMediaByHash(make([]byte, 20))
	if err != nil {
		return err
	}
	if mediaKey != "" {
		return errors.New("unexpected media match for validation hash")
	}
	return nil
}

func upsertCredential(credential string) error {
	values, err := url.ParseQuery(credential)
	if err != nil {
		return fmt.Errorf("parse generated credential: %w", err)
	}
	email := values.Get("Email")

	// Store sensitive credential in OS keyring
	if kr := GetGlobalKeyring(); kr != nil {
		if err := kr.StoreCredential(email, credential); err == nil {
			MarkAccountInKeyring(email, true)
		}
	}

	configMu.Lock()
	defer configMu.Unlock()

	previousCredentials := append([]string(nil), AppConfig.Account.Credentials...)
	previousSelected := AppConfig.Account.Selected
	replaced := false
	for index, existing := range AppConfig.Account.Credentials {
		existingValues, parseErr := url.ParseQuery(existing)
		if parseErr == nil && strings.EqualFold(existingValues.Get("Email"), email) {
			AppConfig.Account.Credentials[index] = credential
			replaced = true
			break
		}
	}
	if !replaced {
		AppConfig.Account.Credentials = append(AppConfig.Account.Credentials, credential)
	}
	AppConfig.Account.Selected = email

	if err := saveAppConfigLocked(); err != nil {
		AppConfig.Account.Credentials = previousCredentials
		AppConfig.Account.Selected = previousSelected
		return err
	}
	return nil
}
