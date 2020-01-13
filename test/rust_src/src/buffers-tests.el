;;; buffers-tests.el --- tests for buffers.rs functions -*- lexical-binding: t -*-

;;; Code:

(require 'ert)

(ert-deftest test-buffer-base-buffer-indirect ()
  (let* ((base (get-buffer-create "base"))
         (ind-buf (make-indirect-buffer base "indbuf")))
    (should (eq (buffer-base-buffer ind-buf) base))))

(ert-deftest test-buffer-base-buffer-non-indirect ()
  (let ((buf (get-buffer-create "buf")))
    (should (eq (buffer-base-buffer buf) nil))))

(ert-deftest test-buffer-overlay-properties ()
  "Tests the overlay-properties function"
  (should-error (eval '(overlay-properties)) :type 'wrong-number-of-arguments)
  (should-error (eval '(overlay-properties "ab")) :type 'wrong-type-argument)
  (let ((overlay (make-overlay 1 1)))
    (should (null (overlay-properties overlay)))
    (overlay-put overlay 'priority 2)
    (should (equal (overlay-properties overlay) '(priority 2)))))

(ert-deftest test-delete-overlay ()
  (let ((buf (get-buffer-create "test-delete-overlay")))
    (with-current-buffer buf
      (overlay-put (make-overlay (point-min) (point-max)) 'test "test")
      (should (= (length (overlays-in (point-min) (point-max))) 1))
      (delete-overlay (car (overlays-in (point-min) (point-max)))))
      (should (eq (overlays-in (point-min) (point-max)) nil))))

(ert-deftest test-delete-all-overlays ()
  (let ((buf (get-buffer-create "test-delete-all-overlays")))
    (with-current-buffer buf
      (overlay-put (make-overlay (point-min) (point-max)) 'test "test")
      (overlay-put (make-overlay (point-min) (point-max)) 'test "test")
      (should (= (length (overlays-in (point-min) (point-max))) 2))
      (delete-all-overlays)
      (should (eq (overlays-in (point-min) (point-max)) nil)))))

(ert-deftest test-erase-buffer ()
  (let ((buf (get-buffer-create "test-erase-buffer")))
    (with-current-buffer buf
      (insert "test")
      (erase-buffer)
      (should (string= (buffer-string) ""))
      (let (pos)
        (insert "test")
        (setq pos (point))
        (insert "narrowed")
        (narrow-to-region pos (point-max))
        (erase-buffer)
        ;; ensure widen is called
        (widen)
        (should (string= (buffer-string) ""))))))

(ert-deftest test-buffer-list-for-frame-is-unique ()
  (get-buffer-create "foo")
  (get-buffer-create "bar")
  (get-buffer-create "baz")
  (let ((the-buffers (buffer-list (selected-frame))))
    (should (equal (delq nil (delete-dups the-buffers))
                   the-buffers))))

(provide 'buffers-tests)

;;; buffers-tests.el ends here
