;;; mml-sec.el --- A package with security functions for MML documents

;; Copyright (C) 2000-2019 Free Software Foundation, Inc.

;; Author: Simon Josefsson <simon@josefsson.org>

;; This file is part of GNU Emacs.

;; GNU Emacs is free software: you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation, either version 3 of the License, or
;; (at your option) any later version.

;; GNU Emacs is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
;; GNU General Public License for more details.

;; You should have received a copy of the GNU General Public License
;; along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.

;;; Commentary:

;;; Code:

(eval-when-compile (require 'cl))

(require 'gnus-util)
(require 'epg)
(require 'epa)
(require 'password-cache)
(require 'mm-encode)

(autoload 'mail-strip-quoted-names "mail-utils")
(autoload 'mml2015-sign "mml2015")
(autoload 'mml2015-encrypt "mml2015")
(autoload 'mml1991-sign "mml1991")
(autoload 'mml1991-encrypt "mml1991")
(autoload 'message-fetch-field "message")
(autoload 'message-goto-body "message")
(autoload 'message-options-get "message")
(autoload 'mml-insert-tag "mml")
(autoload 'mml-smime-sign "mml-smime")
(autoload 'mml-smime-encrypt "mml-smime")
(autoload 'mml-smime-sign-query "mml-smime")
(autoload 'mml-smime-encrypt-query "mml-smime")
(autoload 'mml-smime-verify "mml-smime")
(autoload 'mml-smime-verify-test "mml-smime")
(autoload 'epa--select-keys "epa")
(autoload 'message-options-get "message")
(autoload 'message-options-set "message")

(declare-function message-options-set "message" (symbol value))

(defvar mml-sign-alist
  '(("smime"     mml-smime-sign-buffer     mml-smime-sign-query)
    ("pgp"       mml-pgp-sign-buffer       list)
    ("pgpauto"   mml-pgpauto-sign-buffer  list)
    ("pgpmime"   mml-pgpmime-sign-buffer   list))
  "Alist of MIME signer functions.")

(defcustom mml-default-sign-method "pgpmime"
  "Default sign method.
The string must have an entry in `mml-sign-alist'."
  :version "22.1"
  :type '(choice (const "smime")
		 (const "pgp")
		 (const "pgpauto")
		 (const "pgpmime")
		 string)
  :group 'message)

(defvar mml-encrypt-alist
  '(("smime"     mml-smime-encrypt-buffer     mml-smime-encrypt-query)
    ("pgp"       mml-pgp-encrypt-buffer       list)
    ("pgpauto"   mml-pgpauto-sign-buffer  list)
    ("pgpmime"   mml-pgpmime-encrypt-buffer   list))
  "Alist of MIME encryption functions.")

(defcustom mml-default-encrypt-method "pgpmime"
  "Default encryption method.
The string must have an entry in `mml-encrypt-alist'."
  :version "22.1"
  :type '(choice (const "smime")
		 (const "pgp")
		 (const "pgpauto")
		 (const "pgpmime")
		 string)
  :group 'message)

(defcustom mml-signencrypt-style-alist
  '(("smime"   separate)
    ("pgp"     combined)
    ("pgpauto" combined)
    ("pgpmime" combined))
  "Alist specifying if `signencrypt' results in two separate operations or not.
The first entry indicates the MML security type, valid entries include
the strings \"smime\", \"pgp\", and \"pgpmime\".  The second entry is
a symbol `separate' or `combined' where `separate' means that MML signs
and encrypt messages in a two step process, and `combined' means that MML
signs and encrypt the message in one step.

Note that the output generated by using a `combined' mode is NOT
understood by all PGP implementations, in particular PGP version
2 does not support it!  See Info node `(message) Security' for
details."
  :version "22.1"
  :group 'message
  :type '(repeat (list (choice (const :tag "S/MIME" "smime")
			       (const :tag "PGP" "pgp")
			       (const :tag "PGP/MIME" "pgpmime")
			       (string :tag "User defined"))
		       (choice (const :tag "Separate" separate)
			       (const :tag "Combined" combined)))))

(defcustom mml-secure-verbose nil
  "If non-nil, ask the user about the current operation more verbosely."
  :group 'message
  :type 'boolean)

;; FIXME If it's "NOT recommended", why is it the default?
(defcustom mml-secure-cache-passphrase password-cache
  "If t, cache OpenPGP or S/MIME passphrases inside Emacs.
Passphrase caching in Emacs is NOT recommended.  Use gpg-agent instead.
See Info node `(message) Security'."
  :group 'message
  :type 'boolean)

(defcustom mml-secure-passphrase-cache-expiry password-cache-expiry
  "How many seconds the passphrase is cached.
Whether the passphrase is cached at all is controlled by
`mml-secure-cache-passphrase'."
  :group 'message
  :type 'integer)

(defcustom mml-secure-safe-bcc-list nil
  "List of e-mail addresses that are safe to use in Bcc headers.
EasyPG encrypts e-mails to Bcc addresses, and the encrypted e-mail
by default identifies the used encryption keys, giving away the
Bcc'ed identities.  Clearly, this contradicts the original goal of
*blind* copies.
For an academic paper explaining the problem, see URL
`http://crypto.stanford.edu/portia/papers/bb-bcc.pdf'.
Use this variable to specify e-mail addresses whose owners do not
mind if they are identifiable as recipients.  This may be useful if
you use Bcc headers to encrypt e-mails to yourself."
  :version "25.1"
  :group 'message
  :type '(repeat string))

;;; Configuration/helper functions

(defun mml-signencrypt-style (method &optional style)
  "Function for setting/getting the signencrypt-style used.  Takes two
arguments, the method (e.g. \"pgp\") and optionally the mode
\(e.g. combined).  If the mode is omitted, the current value is returned.

For example, if you prefer to use combined sign & encrypt with
smime, putting the following in your Gnus startup file will
enable that behavior:

\(mml-set-signencrypt-style \"smime\" combined)

You can also customize or set `mml-signencrypt-style-alist' instead."
  (let ((style-item (assoc method mml-signencrypt-style-alist)))
    (if style-item
	(if (or (eq style 'separate)
		(eq style 'combined))
	    ;; valid style setting?
	    (setf (second style-item) style)
	  ;; otherwise, just return the current value
	  (second style-item))
      (message "Warning, attempt to set invalid signencrypt style"))))

;;; Security functions

(defun mml-smime-sign-buffer (cont)
  (or (mml-smime-sign cont)
      (error "Signing failed... inspect message logs for errors")))

(defun mml-smime-encrypt-buffer (cont &optional sign)
  (when sign
    (message "Combined sign and encrypt S/MIME not support yet")
    (sit-for 1))
  (or (mml-smime-encrypt cont)
      (error "Encryption failed... inspect message logs for errors")))

(defun mml-pgp-sign-buffer (cont)
  (or (mml1991-sign cont)
      (error "Signing failed... inspect message logs for errors")))

(defun mml-pgp-encrypt-buffer (cont &optional sign)
  (or (mml1991-encrypt cont sign)
      (error "Encryption failed... inspect message logs for errors")))

(defun mml-pgpmime-sign-buffer (cont)
  (or (mml2015-sign cont)
      (error "Signing failed... inspect message logs for errors")))

(defun mml-pgpmime-encrypt-buffer (cont &optional sign)
  (or (mml2015-encrypt cont sign)
      (error "Encryption failed... inspect message logs for errors")))

(defun mml-pgpauto-sign-buffer (cont)
  (message-goto-body)
  (or (if (re-search-backward "Content-Type: *multipart/.*" nil t) ; there must be a better way...
	  (mml2015-sign cont)
	(mml1991-sign cont))
      (error "Encryption failed... inspect message logs for errors")))

(defun mml-pgpauto-encrypt-buffer (cont &optional sign)
  (message-goto-body)
  (or (if (re-search-backward "Content-Type: *multipart/.*" nil t) ; there must be a better way...
	  (mml2015-encrypt cont sign)
	(mml1991-encrypt cont sign))
      (error "Encryption failed... inspect message logs for errors")))

(defun mml-secure-part (method &optional sign)
  (save-excursion
    (let ((tags (funcall (nth 2 (assoc method (if sign mml-sign-alist
						mml-encrypt-alist))))))
      (cond ((re-search-backward
	      "<#\\(multipart\\|part\\|external\\|mml\\)" nil t)
	     (goto-char (match-end 0))
	     (insert (if sign " sign=" " encrypt=") method)
	     (while tags
	       (let ((key (pop tags))
		     (value (pop tags)))
		 (when value
		   ;; Quote VALUE if it contains suspicious characters.
		   (when (string-match "[\"'\\~/*;() \t\n]" value)
		     (setq value (prin1-to-string value)))
		   (insert (format " %s=%s" key value))))))
	    ((or (re-search-backward
		  (concat "^" (regexp-quote mail-header-separator) "\n") nil t)
		 (re-search-forward
		  (concat "^" (regexp-quote mail-header-separator) "\n") nil t))
	     (goto-char (match-end 0))
	     (apply 'mml-insert-tag 'part (cons (if sign 'sign 'encrypt)
						(cons method tags))))
	    (t (error "The message is corrupted. No mail header separator"))))))

(defvar mml-secure-method
  (if (equal mml-default-encrypt-method mml-default-sign-method)
      mml-default-sign-method
    "pgpmime")
  "Current security method.  Internal variable.")

(defun mml-secure-sign (&optional method)
  "Add MML tags to sign this MML part.
Use METHOD if given.  Else use `mml-secure-method' or
`mml-default-sign-method'."
  (interactive)
  (mml-secure-part
   (or method mml-secure-method mml-default-sign-method)
   'sign))

(defun mml-secure-encrypt (&optional method)
  "Add MML tags to encrypt this MML part.
Use METHOD if given.  Else use `mml-secure-method' or
`mml-default-sign-method'."
  (interactive)
  (mml-secure-part
   (or method mml-secure-method mml-default-sign-method)))

(defun mml-secure-sign-pgp ()
  "Add MML tags to PGP sign this MML part."
  (interactive)
  (mml-secure-part "pgp" 'sign))

(defun mml-secure-sign-pgpauto ()
  "Add MML tags to PGP-auto sign this MML part."
  (interactive)
  (mml-secure-part "pgpauto" 'sign))

(defun mml-secure-sign-pgpmime ()
  "Add MML tags to PGP/MIME sign this MML part."
  (interactive)
  (mml-secure-part "pgpmime" 'sign))

(defun mml-secure-sign-smime ()
  "Add MML tags to S/MIME sign this MML part."
  (interactive)
  (mml-secure-part "smime" 'sign))

(defun mml-secure-encrypt-pgp ()
  "Add MML tags to PGP encrypt this MML part."
  (interactive)
  (mml-secure-part "pgp"))

(defun mml-secure-encrypt-pgpmime ()
  "Add MML tags to PGP/MIME encrypt this MML part."
  (interactive)
  (mml-secure-part "pgpmime"))

(defun mml-secure-encrypt-smime ()
  "Add MML tags to S/MIME encrypt this MML part."
  (interactive)
  (mml-secure-part "smime"))

(defun mml-secure-is-encrypted-p ()
  "Check whether secure encrypt tag is present."
  (save-excursion
    (goto-char (point-min))
    (re-search-forward
     (concat "^" (regexp-quote mail-header-separator) "\n"
	     "<#secure[^>]+encrypt")
     nil t)))

(defun mml-secure-bcc-is-safe ()
  "Check whether usage of Bcc is safe (or absent).
Bcc usage is safe in two cases: first, if the current message does
not contain an MML secure encrypt tag;
second, if the Bcc addresses are a subset of `mml-secure-safe-bcc-list'.
In all other cases, ask the user whether Bcc usage is safe.
Raise error if user answers no.
Note that this function does not produce a meaningful return value:
either an error is raised or not."
  (when (mml-secure-is-encrypted-p)
    (let ((bcc (mail-strip-quoted-names (message-fetch-field "bcc"))))
      (when bcc
	(let ((bcc-list (mapcar #'cadr
				(mail-extract-address-components bcc t))))
	  (unless (gnus-subsetp bcc-list mml-secure-safe-bcc-list)
	    (unless (yes-or-no-p "Message for encryption contains Bcc header.\
  This may give away all Bcc'ed identities to all recipients.\
  Are you sure that this is safe?\
  (Customize `mml-secure-safe-bcc-list' to avoid this warning.) ")
	      (error "Aborted"))))))))

;; defuns that add the proper <#secure ...> tag to the top of the message body
(defun mml-secure-message (method &optional modesym)
  (let ((mode (prin1-to-string modesym))
	(tags (append
	       (if (or (eq modesym 'sign)
		       (eq modesym 'signencrypt))
		   (funcall (nth 2 (assoc method mml-sign-alist))))
	       (if (or (eq modesym 'encrypt)
		       (eq modesym 'signencrypt))
		   (funcall (nth 2 (assoc method mml-encrypt-alist))))))
	insert-loc)
    (mml-unsecure-message)
    (save-excursion
      (goto-char (point-min))
      (cond ((re-search-forward
	      (concat "^" (regexp-quote mail-header-separator) "\n") nil t)
	     (goto-char (setq insert-loc (match-end 0)))
	     (unless (looking-at "<#secure")
	       (apply 'mml-insert-tag
		'secure 'method method 'mode mode tags)))
	    (t (error
		"The message is corrupted. No mail header separator"))))
    (when (eql insert-loc (point))
      (forward-line 1))))

(defun mml-unsecure-message ()
  "Remove security related MML tags from message."
  (interactive)
  (save-excursion
    (goto-char (point-max))
    (when (re-search-backward "^<#secure.*>\n" nil t)
      (delete-region (match-beginning 0) (match-end 0)))))


(defun mml-secure-message-sign (&optional method)
  "Add MML tags to sign the entire message.
Use METHOD if given. Else use `mml-secure-method' or
`mml-default-sign-method'."
  (interactive)
  (mml-secure-message
   (or method mml-secure-method mml-default-sign-method)
   'sign))

(defun mml-secure-message-sign-encrypt (&optional method)
  "Add MML tag to sign and encrypt the entire message.
Use METHOD if given. Else use `mml-secure-method' or
`mml-default-sign-method'."
  (interactive)
  (mml-secure-message
   (or method mml-secure-method mml-default-sign-method)
   'signencrypt))

(defun mml-secure-message-encrypt (&optional method)
  "Add MML tag to encrypt the entire message.
Use METHOD if given. Else use `mml-secure-method' or
`mml-default-sign-method'."
  (interactive)
  (mml-secure-message
   (or method mml-secure-method mml-default-sign-method)
   'encrypt))

(defun mml-secure-message-sign-smime ()
  "Add MML tag to encrypt/sign the entire message."
  (interactive)
  (mml-secure-message "smime" 'sign))

(defun mml-secure-message-sign-pgp ()
  "Add MML tag to encrypt/sign the entire message."
  (interactive)
  (mml-secure-message "pgp" 'sign))

(defun mml-secure-message-sign-pgpmime ()
  "Add MML tag to encrypt/sign the entire message."
  (interactive)
  (mml-secure-message "pgpmime" 'sign))

(defun mml-secure-message-sign-pgpauto ()
  "Add MML tag to encrypt/sign the entire message."
  (interactive)
  (mml-secure-message "pgpauto" 'sign))

(defun mml-secure-message-encrypt-smime (&optional dontsign)
  "Add MML tag to encrypt and sign the entire message.
If called with a prefix argument, only encrypt (do NOT sign)."
  (interactive "P")
  (mml-secure-message "smime" (if dontsign 'encrypt 'signencrypt)))

(defun mml-secure-message-encrypt-pgp (&optional dontsign)
  "Add MML tag to encrypt and sign the entire message.
If called with a prefix argument, only encrypt (do NOT sign)."
  (interactive "P")
  (mml-secure-message "pgp" (if dontsign 'encrypt 'signencrypt)))

(defun mml-secure-message-encrypt-pgpmime (&optional dontsign)
  "Add MML tag to encrypt and sign the entire message.
If called with a prefix argument, only encrypt (do NOT sign)."
  (interactive "P")
  (mml-secure-message "pgpmime" (if dontsign 'encrypt 'signencrypt)))

(defun mml-secure-message-encrypt-pgpauto (&optional dontsign)
  "Add MML tag to encrypt and sign the entire message.
If called with a prefix argument, only encrypt (do NOT sign)."
  (interactive "P")
  (mml-secure-message "pgpauto" (if dontsign 'encrypt 'signencrypt)))

;;; Common functionality for mml1991.el, mml2015.el, mml-smime.el

(define-obsolete-variable-alias 'mml1991-signers 'mml-secure-openpgp-signers
  "25.1")
(define-obsolete-variable-alias 'mml2015-signers 'mml-secure-openpgp-signers
  "25.1")
(defcustom mml-secure-openpgp-signers nil
  "A list of your own key ID(s) which will be used to sign OpenPGP messages.
If set, it is added to the setting of `mml-secure-openpgp-sign-with-sender'."
  :group 'mime-security
  :type '(repeat (string :tag "Key ID")))

(define-obsolete-variable-alias 'mml-smime-signers 'mml-secure-smime-signers
  "25.1")
(defcustom mml-secure-smime-signers nil
  "A list of your own key ID(s) which will be used to sign S/MIME messages.
If set, it is added to the setting of `mml-secure-smime-sign-with-sender'."
  :group 'mime-security
  :type '(repeat (string :tag "Key ID")))

(define-obsolete-variable-alias
  'mml1991-encrypt-to-self 'mml-secure-openpgp-encrypt-to-self "25.1")
(define-obsolete-variable-alias
  'mml2015-encrypt-to-self 'mml-secure-openpgp-encrypt-to-self "25.1")
(defcustom mml-secure-openpgp-encrypt-to-self nil
  "List of own key ID(s) or t; determines additional recipients with OpenPGP.
If t, also encrypt to key for message sender; if list, encrypt to those keys.
With this variable, you can ensure that you can decrypt your own messages.
Alternatives to this variable include Bcc'ing the message to yourself or
using the encrypt-to or hidden-encrypt-to option in gpg.conf (see man gpg(1)).
Note that this variable and the encrypt-to option give away your identity
for *every* encryption without warning, which is not what you want if you are
using, e.g., remailers.
Also, use of Bcc gives away your identity for *every* encryption without
warning, which is a bug, see:
https://debbugs.gnu.org/cgi/bugreport.cgi?bug=18718"
  :group 'mime-security
  :type '(choice (const :tag "None" nil)
		 (const :tag "From address" t)
		 (repeat (string :tag "Key ID"))))

(define-obsolete-variable-alias
  'mml-smime-encrypt-to-self 'mml-secure-smime-encrypt-to-self "25.1")
(defcustom mml-secure-smime-encrypt-to-self nil
  "List of own key ID(s) or t; determines additional recipients with S/MIME.
If t, also encrypt to key for message sender; if list, encrypt to those keys.
With this variable, you can ensure that you can decrypt your own messages.
Alternatives to this variable include Bcc'ing the message to yourself or
using the encrypt-to option in gpgsm.conf (see man gpgsm(1)).
Note that this variable and the encrypt-to option give away your identity
for *every* encryption without warning, which is not what you want if you are
using, e.g., remailers.
Also, use of Bcc gives away your identity for *every* encryption without
warning, which is a bug, see:
https://debbugs.gnu.org/cgi/bugreport.cgi?bug=18718"
  :group 'mime-security
  :type '(choice (const :tag "None" nil)
		 (const :tag "From address" t)
		 (repeat (string :tag "Key ID"))))

(define-obsolete-variable-alias
  'mml2015-sign-with-sender 'mml-secure-openpgp-sign-with-sender "25.1")
;mml1991-sign-with-sender did never exist.
(defcustom mml-secure-openpgp-sign-with-sender nil
  "If t, use message sender to find an OpenPGP key to sign with."
  :group 'mime-security
  :type 'boolean)

(define-obsolete-variable-alias
  'mml-smime-sign-with-sender 'mml-secure-smime-sign-with-sender "25.1")
(defcustom mml-secure-smime-sign-with-sender nil
  "If t, use message sender to find an S/MIME key to sign with."
  :group 'mime-security
  :type 'boolean)

(define-obsolete-variable-alias
  'mml2015-always-trust 'mml-secure-openpgp-always-trust "25.1")
;mml1991-always-trust did never exist.
(defcustom mml-secure-openpgp-always-trust t
  "If t, skip key validation of GnuPG on encryption."
  :group 'mime-security
  :type 'boolean)

(defcustom mml-secure-fail-when-key-problem nil
  "If t, raise an error if some key is missing or several keys exist.
Otherwise, ask the user."
  :version "25.1"
  :group 'mime-security
  :type 'boolean)

(defcustom mml-secure-key-preferences
  '((OpenPGP (sign) (encrypt)) (CMS (sign) (encrypt)))
  "Protocol- and usage-specific fingerprints of preferred keys.
This variable is only relevant if a recipient owns multiple key pairs (for
encryption) or you own multiple key pairs (for signing).  In such cases,
you will be asked which key(s) should be used, and your choice can be
customized in this variable."
  :version "25.1"
  :group 'mime-security
  :type '(alist :key-type (symbol :tag "Protocol") :value-type
		(alist :key-type (symbol :tag "Usage") :value-type
		       (alist :key-type (string :tag "Name") :value-type
			      (repeat (string :tag "Fingerprint"))))))

(defun mml-secure-cust-usage-lookup (context usage)
  "Return preferences for CONTEXT and USAGE."
  (let* ((protocol (epg-context-protocol context))
	 (protocol-prefs (cdr (assoc protocol mml-secure-key-preferences))))
    (assoc usage protocol-prefs)))

(defun mml-secure-cust-fpr-lookup (context usage name)
  "Return fingerprints of preferred keys for CONTEXT, USAGE, and NAME."
  (let* ((usage-prefs (mml-secure-cust-usage-lookup context usage))
	 (fprs (assoc name (cdr usage-prefs))))
    (when fprs
      (cdr fprs))))

(defun mml-secure-cust-record-keys (context usage name keys &optional save)
  "For CONTEXT, USAGE, and NAME record fingerprint(s) of KEYS.
If optional SAVE is not nil, save customized fingerprints.
Return keys."
  (assert keys)
  (let* ((usage-prefs (mml-secure-cust-usage-lookup context usage))
	 (curr-fprs (cdr (assoc name (cdr usage-prefs))))
	 (key-fprs (mapcar 'mml-secure-fingerprint keys))
	 (new-fprs (cl-union curr-fprs key-fprs :test 'equal)))
    (if curr-fprs
	(setcdr (assoc name (cdr usage-prefs)) new-fprs)
      (setcdr usage-prefs (cons (cons name new-fprs) (cdr usage-prefs))))
    (when save
	(customize-save-variable
	 'mml-secure-key-preferences mml-secure-key-preferences))
    keys))

(defun mml-secure-cust-remove-keys (context usage name)
  "Remove keys for CONTEXT, USAGE, and NAME.
Return t if a customization for NAME was present (and has been removed)."
  (let* ((usage-prefs (mml-secure-cust-usage-lookup context usage))
	 (current (assoc name usage-prefs)))
    (when current
      (setcdr usage-prefs (remove current (cdr usage-prefs)))
      t)))

(defvar mml-secure-secret-key-id-list nil)

(defun mml-secure-add-secret-key-id (key-id)
  "Record KEY-ID in list of secret keys."
  (add-to-list 'mml-secure-secret-key-id-list key-id))

(defun mml-secure-clear-secret-key-id-list ()
  "Remove passwords from cache and clear list of secret keys."
  ;; Loosely based on code inside mml2015-epg-encrypt,
  ;; mml2015-epg-clear-decrypt, and mml2015-epg-decrypt
  (dolist (key-id mml-secure-secret-key-id-list nil)
    (password-cache-remove key-id))
  (setq mml-secure-secret-key-id-list nil))

(defvar mml1991-cache-passphrase)
(defvar mml1991-passphrase-cache-expiry)

(defun mml-secure-cache-passphrase-p (protocol)
  "Return t if OpenPGP or S/MIME passphrases should be cached for PROTOCOL.
Passphrase caching in Emacs is NOT recommended.  Use gpg-agent instead."
  (or (and (eq 'OpenPGP protocol)
	   (or mml-secure-cache-passphrase
	       (and (boundp 'mml2015-cache-passphrase)
		    mml2015-cache-passphrase)
	       (and (boundp 'mml1991-cache-passphrase)
		    mml1991-cache-passphrase)))
      (and (eq 'CMS protocol)
	   (or mml-secure-cache-passphrase
	       (and (boundp 'mml-smime-cache-passphrase)
		    mml-smime-cache-passphrase)))))

(defun mml-secure-cache-expiry-interval (protocol)
  "Return time in seconds to cache passphrases for PROTOCOL.
Passphrase caching in Emacs is NOT recommended.  Use gpg-agent instead."
  (or (and (eq 'OpenPGP protocol)
	   (or (and (boundp 'mml2015-passphrase-cache-expiry)
		    mml2015-passphrase-cache-expiry)
	       (and (boundp 'mml1991-passphrase-cache-expiry)
		    mml1991-passphrase-cache-expiry)
	       mml-secure-passphrase-cache-expiry))
      (and (eq 'CMS protocol)
	   (or (and (boundp 'mml-smime-passphrase-cache-expiry)
		    mml-smime-passphrase-cache-expiry)
	       mml-secure-passphrase-cache-expiry))))

(defun mml-secure-passphrase-callback (context key-id standard)
  "Ask for passphrase in CONTEXT for KEY-ID for STANDARD.
The passphrase is read and cached."
  ;; Based on mml2015-epg-passphrase-callback.
  (if (eq key-id 'SYM)
      (epa-passphrase-callback-function context key-id nil)
    (let* ((password-cache-key-id
	    (if (eq key-id 'PIN)
		"PIN"
	       key-id))
	   (entry (assoc key-id epg-user-id-alist))
	   (passphrase
	    (password-read
	     (if (eq key-id 'PIN)
		 "Passphrase for PIN: "
	       (if entry
		   (format "Passphrase for %s %s: " key-id (cdr entry))
		 (format "Passphrase for %s: " key-id)))
	     ;; TODO: With mml-smime.el, password-cache-key-id is not passed
	     ;; as argument to password-read.
	     ;; Is that on purpose?  If so, the following needs to be placed
	     ;; inside an if statement.
	     password-cache-key-id)))
      (when passphrase
	(let ((password-cache-expiry (mml-secure-cache-expiry-interval
				      (epg-context-protocol context))))
	  ;; FIXME test passphrase works before caching it.
	  (password-cache-add password-cache-key-id passphrase))
	(mml-secure-add-secret-key-id password-cache-key-id)
	(copy-sequence passphrase)))))

(defun mml-secure-check-user-id (key recipient)
  "Check whether KEY has a non-revoked, non-expired UID for RECIPIENT."
  ;; Based on mml2015-epg-check-user-id.
  (let ((uids (epg-key-user-id-list key)))
    (catch 'break
      (dolist (uid uids nil)
	(if (and (stringp (epg-user-id-string uid))
                 (car (mail-header-parse-address
                       (epg-user-id-string uid)))
		 (equal (downcase (car (mail-header-parse-address
					(epg-user-id-string uid))))
			(downcase (car (mail-header-parse-address
					recipient))))
		 (not (memq (epg-user-id-validity uid)
			    '(revoked expired))))
	    (throw 'break t))))))

(defun mml-secure-secret-key-exists-p (context subkey)
  "Return t if keyring for CONTEXT contains secret key for public SUBKEY."
  (let* ((fpr (epg-sub-key-fingerprint subkey))
	 (candidates (epg-list-keys context fpr 'secret))
	 (candno (length candidates)))
    ;; If two or more subkeys with the same fingerprint exist, something is
    ;; terribly wrong.
    (when (>= candno 2)
      (error "Found %d secret keys with same fingerprint %s" candno fpr))
    (= 1 candno)))

(defun mml-secure-check-sub-key (context key usage &optional fingerprint)
  "Check whether in CONTEXT the public KEY has a usable subkey for USAGE.
This is the case if KEY is not disabled, and there is a subkey for
USAGE that is neither revoked nor expired.  Additionally, if optional
FINGERPRINT is present and if it is not the primary key's fingerprint, then
the returned subkey must have that FINGERPRINT.  FINGERPRINT must consist of
hexadecimal digits only (no leading \"0x\" allowed).
If USAGE is not `encrypt', then additionally an appropriate secret key must
be present in the keyring."
  ;; Based on mml2015-epg-check-sub-key, extended by
  ;; - check for secret keys if usage is not 'encrypt and
  ;; - check for new argument FINGERPRINT.
  (let* ((subkeys (epg-key-sub-key-list key))
	 (primary (car subkeys))
	 (fpr (epg-sub-key-fingerprint primary)))
    ;; The primary key will be marked as disabled, when the entire
    ;; key is disabled (see 12 Field, Format of colon listings, in
    ;; gnupg/doc/DETAILS)
    (unless (memq 'disabled (epg-sub-key-capability primary))
      (catch 'break
	(dolist (subkey subkeys nil)
	  (if (and (memq usage (epg-sub-key-capability subkey))
		   (not (memq (epg-sub-key-validity subkey)
			      '(revoked expired)))
		   (or (eq 'encrypt usage) ; Encryption works with public key.
		       ;; In contrast, signing requires secret key.
		       (mml-secure-secret-key-exists-p context subkey))
		   (or (not fingerprint)
		       (string-match-p (concat fingerprint "$") fpr)
		       (string-match-p (concat fingerprint "$")
				       (epg-sub-key-fingerprint subkey))))
	      (throw 'break t)))))))

(defun mml-secure-find-usable-keys (context name usage &optional justone)
  "In CONTEXT return a list of keys for NAME and USAGE.
If USAGE is `encrypt' public keys are returned, otherwise secret ones.
Only non-revoked and non-expired keys are returned whose primary key is
not disabled.
NAME can be an e-mail address or a key ID.
If NAME just consists of hexadecimal digits (possibly prefixed by \"0x\"), it
is treated as key ID for which at most one key must exist in the keyring.
Otherwise, NAME is treated as user ID, for which no keys are returned if it
is expired or revoked.
If optional JUSTONE is not nil, return the first key instead of a list."
  (let* ((keys (epg-list-keys context name))
	 (iskeyid (string-match "\\(0x\\)?\\([0-9a-fA-F]\\{8,\\}\\)" name))
	 (fingerprint (match-string 2 name))
	 result)
    (when (and iskeyid (>= (length keys) 2))
      (error
       "Name %s (for %s) looks like a key ID but multiple keys found"
       name usage))
    (catch 'break
      (dolist (key keys result)
	(if (and (or iskeyid
		     (mml-secure-check-user-id key name))
		 (mml-secure-check-sub-key context key usage fingerprint))
	    (if justone
		(throw 'break key)
	      (push key result)))))))

(defun mml-secure-select-preferred-keys (context names usage)
  "Return list of preferred keys in CONTEXT for NAMES and USAGE.
This inspects the keyrings to find keys for each name in NAMES.  If several
keys are found for a name, `mml-secure-select-keys' is used to look for
customized preferences or have the user select preferable ones.
When `mml-secure-fail-when-key-problem' is t, fail with an error in
case of missing, outdated, or multiple keys."
  ;; Loosely based on code appearing inside mml2015-epg-sign and
  ;; mml2015-epg-encrypt.
  (apply
   #'nconc
   (mapcar
    (lambda (name)
      (let* ((keys (mml-secure-find-usable-keys context name usage))
	     (keyno (length keys)))
	(cond ((= 0 keyno)
	       (when (or mml-secure-fail-when-key-problem
			 (not (y-or-n-p
			       (format "No %s key for %s; skip it? "
				       usage name))))
		 (error "No %s key for %s" usage name)))
	      ((= 1 keyno) keys)
	      (t (mml-secure-select-keys context name keys usage)))))
    names)))

(defun mml-secure-fingerprint (key)
  "Return fingerprint for public KEY."
  (epg-sub-key-fingerprint (car (epg-key-sub-key-list key))))

(defun mml-secure-filter-keys (keys fprs)
  "Filter KEYS to subset with fingerprints in FPRS."
  (when keys
    (if (member (mml-secure-fingerprint (car keys)) fprs)
	(cons (car keys) (mml-secure-filter-keys (cdr keys) fprs))
      (mml-secure-filter-keys (cdr keys) fprs))))

(defun mml-secure-normalize-cust-name (name)
  "Normalize NAME to be used for customization.
Currently, remove ankle brackets."
  (if (string-match "^<\\(.*\\)>$" name)
      (match-string 1 name)
    name))

(defun mml-secure-select-keys (context name keys usage)
  "In CONTEXT for NAME select among KEYS for USAGE.
KEYS should be a list with multiple entries.
NAME is normalized first as customized keys are inspected.
When `mml-secure-fail-when-key-problem' is t, fail with an error in case of
outdated or multiple keys."
  (let* ((nname (mml-secure-normalize-cust-name name))
	 (fprs (mml-secure-cust-fpr-lookup context usage nname))
	 (usable-fprs (mapcar 'mml-secure-fingerprint keys)))
    (if fprs
	(if (gnus-subsetp fprs usable-fprs)
	    (mml-secure-filter-keys keys fprs)
	  (mml-secure-cust-remove-keys context usage nname)
	  (let ((diff (gnus-setdiff fprs usable-fprs)))
	    (if mml-secure-fail-when-key-problem
		(error "Customization of %s keys for %s outdated" usage nname)
	      (mml-secure-select-keys-1
	       context nname keys usage (format "\
Customized keys
 (%s)
for %s not available any more.
Select anew.  "
					       diff nname)))))
      (if mml-secure-fail-when-key-problem
	  (error "Multiple %s keys for %s" usage nname)
	(mml-secure-select-keys-1
	 context nname keys usage (format "\
Multiple %s keys for:
 %s
Select preferred one(s).  "
					 usage nname))))))

(defun mml-secure-select-keys-1 (context name keys usage message)
  "In CONTEXT for NAME let user select among KEYS for USAGE, showing MESSAGE.
Return selected keys."
  (let* ((selected (epa--select-keys message keys))
	 (selno (length selected))
	 ;; TODO: y-or-n-p does not always resize the echo area but may
	 ;; truncate the message.  Why?  The following does not help.
	 ;; yes-or-no-p shows full message, though.
	 (message-truncate-lines nil))
    (if selected
	(if (y-or-n-p
	     (format "%d %s key(s) selected.  Store for %s? "
		     selno usage name))
	    (mml-secure-cust-record-keys context usage name selected 'save)
	  selected)
      (unless (y-or-n-p
	       (format "No %s key for %s; skip it? " usage name))
	(error "No %s key for %s" usage name)))))

(defun mml-secure-signer-names (protocol sender)
  "Determine signer names for PROTOCOL and message from SENDER.
Returned names may be e-mail addresses or key IDs and are determined based
on `mml-secure-openpgp-signers' and `mml-secure-openpgp-sign-with-sender' with
OpenPGP or `mml-secure-smime-signers' and `mml-secure-smime-sign-with-sender'
with S/MIME."
  (if (eq 'OpenPGP protocol)
      (append mml-secure-openpgp-signers
	      (if (and mml-secure-openpgp-sign-with-sender sender)
		  (list (concat "<" sender ">"))))
    (append mml-secure-smime-signers
	    (if (and mml-secure-smime-sign-with-sender sender)
		(list (concat "<" sender ">"))))))

(defun mml-secure-signers (context signer-names)
  "Determine signing keys in CONTEXT from SIGNER-NAMES.
If `mm-sign-option' is `guided', the user is asked to choose.
Otherwise, `mml-secure-select-preferred-keys' is used."
  ;; Based on code appearing inside mml2015-epg-sign and
  ;; mml2015-epg-encrypt.
  (if (eq mm-sign-option 'guided)
      (epa-select-keys context "\
Select keys for signing.
If no one is selected, default secret key is used.  "
		       signer-names t)
    (mml-secure-select-preferred-keys context signer-names 'sign)))

(defun mml-secure-self-recipients (protocol sender)
  "Determine additional recipients based on encrypt-to-self variables.
PROTOCOL specifies OpenPGP or S/MIME for a message from SENDER."
  (let ((encrypt-to-self
	 (if (eq 'OpenPGP protocol)
	     mml-secure-openpgp-encrypt-to-self
	   mml-secure-smime-encrypt-to-self)))
    (when encrypt-to-self
      (if (listp encrypt-to-self)
	  encrypt-to-self
	(list sender)))))

(defun mml-secure-recipients (protocol context config sender)
  "Determine encryption recipients.
PROTOCOL specifies OpenPGP or S/MIME with matching CONTEXT and CONFIG
for a message from SENDER."
  ;; Based on code appearing inside mml2015-epg-encrypt.
  (let ((recipients
	 (apply #'nconc
		(mapcar
		 (lambda (recipient)
		   (or (epg-expand-group config recipient)
		       (list (concat "<" recipient ">"))))
		 (split-string
		  (or (message-options-get 'message-recipients)
		      (message-options-set 'message-recipients
					   (read-string "Recipients: ")))
		  "[ \f\t\n\r\v,]+")))))
    (nconc recipients (mml-secure-self-recipients protocol sender))
    (if (eq mm-encrypt-option 'guided)
	(setq recipients
	      (epa-select-keys context "\
Select recipients for encryption.
If no one is selected, symmetric encryption will be performed.  "
			       recipients))
      (setq recipients
	    (mml-secure-select-preferred-keys context recipients 'encrypt))
      (unless recipients
	(error "No recipient specified")))
    recipients))

(defun mml-secure-epg-encrypt (protocol cont &optional sign)
  ;; Based on code appearing inside mml2015-epg-encrypt.
  (let* ((context (epg-make-context protocol))
	 (config (epg-find-configuration 'OpenPGP))
	 (sender (message-options-get 'message-sender))
	 (recipients (mml-secure-recipients protocol context config sender))
	 (signer-names (mml-secure-signer-names protocol sender))
	 cipher signers)
    (when sign
      (setq signers (mml-secure-signers context signer-names))
      (setf (epg-context-signers context) signers))
    (when (eq 'OpenPGP protocol)
      (setf (epg-context-armor context) t)
      (setf (epg-context-textmode context) t))
    (when (mml-secure-cache-passphrase-p protocol)
      (epg-context-set-passphrase-callback
       context
       (cons 'mml-secure-passphrase-callback protocol)))
    (condition-case error
	(setq cipher
	      (if (eq 'OpenPGP protocol)
		  (epg-encrypt-string context (buffer-string) recipients sign
				      mml-secure-openpgp-always-trust)
		(epg-encrypt-string context (buffer-string) recipients))
	      mml-secure-secret-key-id-list nil)
      (error
       (mml-secure-clear-secret-key-id-list)
       (signal (car error) (cdr error))))
    cipher))

(defun mml-secure-epg-sign (protocol mode)
  ;; Based on code appearing inside mml2015-epg-sign.
  (let* ((context (epg-make-context protocol))
	 (sender (message-options-get 'message-sender))
	 (signer-names (mml-secure-signer-names protocol sender))
	 (signers (mml-secure-signers context signer-names))
	 signature micalg)
    (when (eq 'OpenPGP protocol)
      (setf (epg-context-armor context) t)
      (setf (epg-context-textmode context) t))
    (setf (epg-context-signers context) signers)
    (when (mml-secure-cache-passphrase-p protocol)
      (epg-context-set-passphrase-callback
       context
       (cons 'mml-secure-passphrase-callback protocol)))
    (condition-case error
	(setq signature
	      (if (eq 'OpenPGP protocol)
		  (epg-sign-string context (buffer-string) mode)
		(epg-sign-string context
				 (replace-regexp-in-string
				  "\n" "\r\n" (buffer-string))
				 t))
	      mml-secure-secret-key-id-list nil)
      (error
       (mml-secure-clear-secret-key-id-list)
       (signal (car error) (cdr error))))
    (if (epg-context-result-for context 'sign)
	(setq micalg (epg-new-signature-digest-algorithm
		      (car (epg-context-result-for context 'sign)))))
    (cons signature micalg)))

(provide 'mml-sec)

;;; mml-sec.el ends here
