;;; remacs-helpers.el -- tools for Remacs development
;;; Commentary:

;; This is a collection of tools to help developers working with Remacs source.

(require 's)

;;; Code:

(defun remacs-helpers/ignored-type-part-p (input)
  "Predicate to indicate if INPUT is part of a C type ignored in Rust."
  (string= input "struct"))

(defun remacs-helpers/make-rust-args-from-C-worker (input)
  "Transform function C arguments INPUT into Rust style arguments."
  (mapconcat (lambda (arg) (let* ((pieces (cl-remove-if 'remacs-helpers/ignored-type-part-p
                                                        (split-string (string-trim arg) " ")))
                                  (name (s-append ":" (car (last pieces))))
                                  (rest (butlast pieces)))
                             (if (s-starts-with? "*" name)
                                 (s-join " " (cons (s-chop-prefix "*" name)
                                                   (cons "*mut" rest)))
                               (s-join " " (cons name rest)))))
             (split-string input ",")
             ", "))

(defun remacs-helpers/make-rust-args-from-C (string &optional from to)
  "Transform provided STRING or region indicated by FROM and TO into Rust style arguments."
  (interactive
   (if (use-region-p)
       (list nil (region-beginning) (region-end))
     (let ((bds (bounds-of-thing-at-point 'paragraph)) )
       (list nil (car bds) (cdr bds)) ) ) )

  (let* ((input (or string (buffer-substring-no-properties from to)))
         (output (remacs-helpers/make-rust-args-from-C-worker input)))
    (if string
        output
      (save-excursion
        (delete-region from to)
        (goto-char from)
        (insert output) )) ) )

(provide 'remacs-helpers)

;;; remacs-helpers.el ends here
