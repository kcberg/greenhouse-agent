const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');
const HtmlMinimizerPlugin = require("html-minimizer-webpack-plugin");
const TerserPlugin = require("terser-webpack-plugin");
const MiniCssExtractPlugin = require("mini-css-extract-plugin");
const webpack = require("webpack")

module.exports = (env) => {

    console.log(`env: ${env}`);

    return {
        mode: 'development',
        entry: './src/js/main.js',
        output: {
            // filename: "main.js",
            filename: '[name].[contenthash].js',
            path: path.resolve(__dirname, "dist"),
        },
        plugins: [
            new webpack.DefinePlugin({
                'process.env.API_HOST': JSON.stringify(process.env.API_HOST || 'http://placeholder')
            }),
            new HtmlWebpackPlugin({
                template: 'src/index.html',
                minify: {
                    collapseWhitespace: true,
                    keepClosingSlash: true,
                    removeComments: true,
                    removeRedundantAttributes: true,
                    removeScriptTypeAttributes: true,
                    removeStyleLinkTypeAttributes: true,
                    useShortDoctype: true
                }
            }),
            new MiniCssExtractPlugin({
                linkType: "text/css"
            })
        ],
        optimization: {
            minimize: true,
            minimizer: [
                new HtmlMinimizerPlugin(),
                new TerserPlugin(),
            ],
        },
        devServer: {
            static: path.resolve(__dirname, 'dist'),
            port: 8080,
            hot: true,
            // headers: {
            //   "Access-Control-Allow-Origin": "*",
            //   "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, PATCH, OPTIONS",
            //   "Access-Control-Allow-Headers": "X-Requested-With, x-requested-with, content-type, Authorization"
            // }
        },
        module: {
            rules: [
                {
                    test: /\.(js)$/,
                    exclude: /node_modules/,
                    use: ['babel-loader']
                },
                {
                    test: /\.(scss)$/,
                    use: [
                        {
                            loader: MiniCssExtractPlugin.loader,
                        },
                        {
                            // Interprets `@import` and `url()` like `import/require()` and will resolve them
                            loader: 'css-loader'
                        },
                        {
                            // Loader for webpack to process CSS with PostCSS
                            loader: 'postcss-loader',
                            options: {
                                postcssOptions: {
                                    plugins: function () {
                                        return [
                                            require('autoprefixer')
                                        ];
                                    }
                                }
                            }
                        },
                        {
                            // Loads a SASS/SCSS file and compiles it to CSS
                            loader: 'sass-loader'
                        }
                    ]
                },
                {
                    test: /\.html$/,
                    loader: 'html-loader',
                },
                // {
                //     test: /\.woff(2)?(\?v=[0-9]\.[0-9]\.[0-9])?$/,
                //     include: path.resolve(__dirname, './node_modules/bootstrap-icons/font/fonts'),
                //     use: {
                //         loader: 'file-loader',
                //         options: {
                //             name: '[name].[ext]',
                //             outputPath: 'webfonts',
                //             publicPath: '../webfonts',
                //         },
                //     }
                // }
            ]
        }
    }
}
