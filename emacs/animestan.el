;;; animestan.el --- shared core for the Animestan Emacs wrapper -*- lexical-binding: t; -*-
;; Copyright (C) 2026 M.R. Siavash Katebzadeh <mr@katebzadeh.xyz>
;;
;; This program is free software: you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation, either version 3 of the License, or
;; (at your option) any later version.
;;
;; This program is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
;; GNU General Public License for more details.
;;
;; You should have received a copy of the GNU General Public License
;; along with this program. If not, see <https://www.gnu.org/licenses/>.

(require 'cl-lib)
(require 'subr-x)

(defgroup animestan nil
  "Emacs wrapper for the animestan CLI."
  :group 'applications
  :prefix "animestan/")

(defcustom animestan/cli-program "animestan-cli"
  "Path (or command name) for the animestan CLI binary."
  :type 'string
  :group 'animestan)

(defvar animestan/search-history nil
  "Minibuffer history for `animestan/search'.")

(defvar animestan/episodes-history nil
  "Minibuffer history for `animestan/episodes'.")

(defvar animestan/bookmarks-history nil
  "Minibuffer history for `animestan/bookmarks'.")


(defun animestan//split-lines (output)
  "Split OUTPUT (string) into non-empty newline-separated lines."
  (let ((trimmed (string-trim (or output ""))))
    (unless (string-empty-p trimmed)
      (split-string trimmed "\n" t))))

(defun animestan//parse-search-output (output)
  "Return search entries parsed from OUTPUT.
Each entry is a `(ID . vector)` suitable for `tabulated-list-entries`."
  (cl-loop for line in (animestan//split-lines output)
           for parts = (mapcar #'string-trim (split-string line "\t"))
           for id = (or (nth 0 parts) "")
           for title = (or (nth 1 parts) "")
           when (and (not (string-empty-p id)) (not (string-empty-p title)))
           collect (list id (vector id title))))

(defun animestan//parse-episodes-output (output)
  "Return episode entries parsed from OUTPUT for the episodes buffer."
  (cl-loop for line in (animestan//split-lines output)
           for parts = (mapcar #'string-trim (split-string line "\t"))
           for id = (or (nth 0 parts) "")
           for number = (or (nth 1 parts) "")
           for title = (or (nth 2 parts) "")
           when (and (not (string-empty-p id)) (not (string-empty-p title)))
           collect (list id (vector id number title))))

(defun animestan//parse-bookmarks-output (output)
  "Return bookmark entries parsed from OUTPUT.
Each entry vector contains anime id, anime title, episode id (optional), episode title (optional)."
  (cl-loop for line in (animestan//split-lines output)
           for parts = (mapcar #'string-trim (split-string line "\t"))
           for anime-id = (or (nth 0 parts) "")
           for anime-title = (or (nth 1 parts) "")
           for episode-id = (or (nth 2 parts) "")
           for episode-title = (or (nth 3 parts) "")
           when (and (not (string-empty-p anime-id)) (not (string-empty-p anime-title)))
           collect (list anime-id (vector anime-id anime-title episode-id episode-title))))

(defun animestan//filter-flag (filter)
  "Return the CLI flag for FILTER." 
  (pcase filter
    ("unwatched" "--unwatched")
    ("in-progress" "--in-progress")
    ("next" "--next")
    ("recent" "--recent")
    (_ nil)))

(defun animestan//run-cli-sync (args)
  "Run the animestan CLI synchronously with ARGS and return output.
Raise an error if the command fails."
  (with-temp-buffer
    (let ((exit-code (apply #'process-file animestan/cli-program nil t nil args)))
      (if (and (integerp exit-code) (zerop exit-code))
          (buffer-string)
        (error "Animestan CLI failed (%s): %s"
               exit-code
               (string-trim (buffer-string)))))))

(defun animestan//read-choice (prompt candidates &optional history)
  "Read a choice from CANDIDATES (alist of display . payload).
Return the payload for the selected entry."
  (let* ((choices (mapcar #'car candidates))
         (selection
          (if (fboundp 'consult--read)
              (consult--read choices :prompt prompt :require-match t :sort nil
                             :history history :category 'animestan)
            (completing-read prompt choices nil t nil history))))
    (cdr (assoc selection candidates))))

(defun animestan//search-candidates (output)
  "Return candidates for search completion from OUTPUT."
  (mapcar (lambda (entry)
            (let* ((id (car entry))
                   (vec (cadr entry))
                   (title (aref vec 1))
                   (label title))
              (cons label (list id title))))
          (animestan//parse-search-output output)))

(defun animestan//episodes-candidates (output)
  "Return candidates for episode completion from OUTPUT."
  (mapcar (lambda (entry)
            (let* ((id (car entry))
                   (vec (cadr entry))
                   (number (aref vec 1))
                   (title (aref vec 2))
                   (label (format "Ep %s — %s" number title)))
              (cons label (list id number title))))
          (animestan//parse-episodes-output output)))

(defun animestan//bookmarks-candidates (output)
  "Return candidates for bookmark completion from OUTPUT."
  (mapcar (lambda (entry)
            (let* ((id (car entry))
                   (vec (cadr entry))
                   (title (aref vec 1))
                   (label title))
              (cons label (list id title))))
          (animestan//parse-bookmarks-output output)))

(defun animestan//play-episode-id (episode-id)
  "Play EPISODE-ID via the CLI without blocking Emacs."
  (let* ((name (format "animestan-play-%s" episode-id))
         (buffer (generate-new-buffer (format "*animestan-play-%s*" episode-id)))
         (proc (start-process name buffer animestan/cli-program "play" episode-id)))
    (set-process-query-on-exit-flag proc nil)
    (set-process-sentinel
     proc
     (lambda (proc _event)
       (when (memq (process-status proc) '(exit signal))
         (message "Playback finished for %s" episode-id)
         (when (buffer-live-p (process-buffer proc))
           (kill-buffer (process-buffer proc))))))
    (message "Launched playback for %s" episode-id)))


(defun animestan/search (&optional query)
  "Search using the CLI and pick a result via completion."
  (interactive)
  (let* ((query (or query (read-string "Search anime: " nil 'animestan/search-history)))
         (output (animestan//run-cli-sync (list "search" query)))
         (candidates (animestan//search-candidates output)))
    (unless candidates
      (user-error "No results for %s" query))
    (pcase-let ((`(,anime-id ,title)
                 (animestan//read-choice "Select anime: " candidates 'animestan/search-history)))
      (animestan/episodes anime-id))))

(defun animestan/bookmarks (&optional filter)
  "List bookmarks via completion, optionally using FILTER."
  (interactive)
  (let* ((filter (or filter
                     (completing-read "Filter: "
                                      '("none" "unwatched" "in-progress" "next" "recent")
                                      nil t nil 'animestan/bookmarks-history "none")))
         (flag (animestan//filter-flag filter))
         (args (append (list "bookmarks" "ls") (when flag (list flag))))
         (output (animestan//run-cli-sync args))
         (candidates (animestan//bookmarks-candidates output)))
    (unless candidates
      (user-error "No bookmarks found"))
    (pcase-let ((`(,anime-id ,title)
                 (animestan//read-choice "Select bookmark: " candidates 'animestan/bookmarks-history)))
      (animestan/episodes anime-id))))

(defun animestan/episodes (&optional anime-id filter)
  "List episodes for ANIME-ID using completion and play the selection.

When FILTER is provided (or with a prefix arg interactively), restrict the
episodes list to the selected playback state." 
  (interactive
   (list nil
         (when current-prefix-arg
           (completing-read "Filter episodes: "
                            '("unwatched" "in-progress" "next" "recent")
                            nil t))))
  (let* ((anime-id (or anime-id
                       (read-string "Anime ID: " nil 'animestan/episodes-history)))
         (flag (animestan//filter-flag filter))
         (args (append (list "episodes" anime-id) (when flag (list flag))))
         (output (animestan//run-cli-sync args))
         (candidates (animestan//episodes-candidates output)))
    (unless candidates
      (user-error "No episodes found for %s" anime-id))
    (pcase-let ((`(,episode-id ,_number ,_title)
                 (animestan//read-choice "Select episode: " candidates 'animestan/episodes-history)))
      (animestan//play-episode-id episode-id))))

(provide 'animestan)
