;;; callint-tests.el --- unit tests for callint.c    -*- lexical-binding: t; -*-

;; Copyright (C) 2018 Free Software Foundation, Inc.

;; Author: Philipp Stephani <phst@google.com>

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

;; Unit tests for src/callint.c.

;;; Code:

(require 'ert)

(ert-deftest call-interactively/incomplete-multibyte-sequence ()
  "Check that Bug#30004 is fixed."
  (let ((data (should-error (call-interactively (lambda () (interactive "\xFF"))))))
    (should
     (equal
      (cdr data)
      '("Invalid control letter `\u00FF' (#o377, #x00ff) in interactive calling string")))))

;;; callint-tests.el ends here
