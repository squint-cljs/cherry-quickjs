(ns choq.deps
  "Grenadine host and dynamic dependency loading for choq."
  (:require ["node:crypto" :as crypto]
            ["node:fs" :as fs]
            ["node:os" :as nos]
            [grenadine.core :as grenadine]
            [grenadine.runtime :as runtime]))

(def host
  {:http-get (fn [url]
               (let [r (js/__httpGetSync url)]
                 {:status (.-status r) :headers {} :body (.-body r)}))
   :bytes->utf8 (fn [b] (.toString (js/Buffer.from b) "utf8"))
   :byte-count (fn [b] (.-byteLength (js/Buffer.from b)))
   :read-bytes (fn [p] (fs/readFileSync p))
   :write-bytes! (fn [p d] (fs/writeFileSync p (js/Buffer.from d)) nil)
   :exists? (fn [p] (fs/existsSync p))
   :mkdirs! (fn [p] (fs/mkdirSync p #js {:recursive true}) nil)
   :delete! (fn [p] (fs/rmSync p #js {:recursive true :force true}) nil)
   :atomic-move! (fn [s d] (fs/renameSync s d) nil)
   :home-dir (fn [] (nos/homedir))
   :getenv (fn [k] (aget js/process.env k))
   :digest (fn [algorithm data]
             (-> (crypto/createHash (name algorithm)) (.update data) (.digest "hex")))
   :run-process (fn [{:keys [args env]}]
                  (let [r (js/__runGitSync (into-array args) (clj->js (or env {})))]
                    {:exit (.-exit r) :out (.-out r) :err (.-err r)}))})

(def basis (atom {}))

(defn- add-roots! [roots]
  (js/__addSourceRoots (into-array roots)))

;; resolution cache, keyed on the requested deps like the classpath
;; cache in deps.clj: canonical input -> source roots
(def ^:private cache-version "1")

(defn- cache-key [deps-map]
  (str cache-version "|" (pr-str (sort-by (comp str first) (:deps deps-map)))))

(defn- cpcache-dir []
  (str (nos/homedir) "/.cache/choq/cpcache"))

(defn- cache-file [ck]
  (let [h (-> (crypto/createHash "sha256") (.update ck) (.digest "hex"))]
    (str (cpcache-dir) "/" h ".json")))

(defn- cached-roots
  "Source roots for ck when the cache entry is fresh, else nil."
  [ck]
  (let [f (cache-file ck)]
    (when (fs/existsSync f)
      (let [entry (js/JSON.parse (fs/readFileSync f "utf8"))
            roots (vec (js/Array.from (.-sourceRoots entry)))]
        (when (and (= ck (.-input entry))
                   (seq roots)
                   (every? #(fs/existsSync %) roots))
          roots)))))

(declare add-deps)

(defn add-mvn-dep
  "Ensure lib at version; backs the mvn: require sugar."
  [lib version]
  (add-deps {:deps {(symbol lib) {:mvn/version version}}}))

(defn add-deps
  "Resolve and install the dependencies in a deps.edn map. Namespaces
  from the installed libraries load with plain require afterwards.
  Pass {:force true} to skip the resolution cache."
  ([deps-map] (add-deps deps-map nil))
  ([deps-map opts]
   (let [ck (cache-key deps-map)]
     (if-let [roots (and (not (:force opts)) (cached-roots ck))]
       (do (add-roots! roots)
           {:source-roots roots :cached true})
       ;; namespaces load straight from the jars, so the classpath jars
       ;; are the source roots and nothing is extracted
       (let [install-fn (fn [libs opts]
                          (let [r (grenadine/install! libs (dissoc opts :source-roots?))]
                            (assoc r :source-roots (:classpath r))))
             result (runtime/add-libs! basis add-roots! (:deps deps-map)
                                       {:host host :install-fn install-fn})
             roots (:source-roots result)]
         (when (seq roots)
           (fs/mkdirSync (cpcache-dir) #js {:recursive true})
           (fs/writeFileSync (cache-file ck)
                             (js/JSON.stringify #js {:input ck
                                                     :sourceRoots (into-array roots)})))
         result)))))
