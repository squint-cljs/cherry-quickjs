(ns choq.deps
  "Grenadine host and dynamic dependency loading for choq."
  (:require ["node:crypto" :as crypto]
            ["node:fs" :as fs]
            ["node:os" :as nos]
            ["node:zlib" :as zlib]
            [clojure.string :as str]
            [grenadine.runtime :as runtime]))

(defn- u32 [dv off] (.getUint32 dv off true))
(defn- u16 [dv off] (.getUint16 dv off true))

(defn- extract-jar!
  "Extract a zip archive to dest. Stored and deflated entries only."
  [jar-path dest]
  (let [buf (fs/readFileSync jar-path)
        ab (.-buffer buf)
        bo (.-byteOffset buf)
        dv (js/DataView. ab bo (.-byteLength buf))
        len (.-byteLength buf)
        eocd (loop [i (- len 22)]
               (cond (neg? i) (throw (js/Error. (str "no end of central directory: " jar-path)))
                     (= 0x06054b50 (u32 dv i)) i
                     :else (recur (dec i))))
        n (u16 dv (+ eocd 10))
        cd-off (u32 dv (+ eocd 16))]
    (loop [off cd-off k 0]
      (when (< k n)
        (when-not (= 0x02014b50 (u32 dv off))
          (throw (js/Error. (str "bad central directory entry: " jar-path))))
        (let [method (u16 dv (+ off 10))
              csize (u32 dv (+ off 20))
              nlen (u16 dv (+ off 28))
              elen (u16 dv (+ off 30))
              clen (u16 dv (+ off 32))
              lho (u32 dv (+ off 42))
              nm (.toString (js/Buffer.from (.subarray buf (+ off 46) (+ off 46 nlen))) "utf8")
              lnlen (u16 dv (+ lho 26))
              lelen (u16 dv (+ lho 28))
              dstart (+ lho 30 lnlen lelen)]
          (when-not (str/ends-with? nm "/")
            (let [compressed (.subarray buf dstart (+ dstart csize))
                  data (if (= 8 method) (zlib/inflateRawSync compressed) compressed)
                  outp (str dest "/" nm)
                  dir (subs outp 0 (.lastIndexOf outp "/"))]
              (fs/mkdirSync dir #js {:recursive true})
              (fs/writeFileSync outp data)))
          (recur (+ off 46 nlen elen clen) (inc k)))))
    nil))

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
   :extract-jar! extract-jar!
   :home-dir (fn [] (nos/homedir))
   :getenv (fn [k] (aget js/process.env k))
   :digest (fn [algorithm data]
             (-> (crypto/createHash (name algorithm)) (.update data) (.digest "hex")))})

(def basis (atom {}))

(defn- add-roots! [roots]
  (js/__addSourceRoots (into-array roots)))

(defn add-deps
  "Resolve and install the dependencies in a deps.edn map. Namespaces
  from the installed libraries load with plain require afterwards."
  [deps-map]
  (runtime/add-libs! basis add-roots! (:deps deps-map) {:host host}))
