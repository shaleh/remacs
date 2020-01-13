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
